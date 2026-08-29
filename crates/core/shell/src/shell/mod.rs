#![allow(dead_code)] // Shell compatibility and diagnostic paths are exercised by tests.

use mesh_core_capability::{CapabilityPolicy, EffectiveCapabilities};
use mesh_core_config::{
    ModuleSettingsOverrides, SettingsNamespaceSchema, SettingsStore, ShellConfig, ShellSettings,
    load_config, resolve_discovery_paths,
};
use mesh_core_debug::{
    BackendRuntimeEntry, DebugDiagnosticEntry, DebugOverlayState, DebugSnapshot, HealthEntry,
    InterfaceEntry, ModuleEntry, ProviderEntry,
};
use mesh_core_diagnostics::DiagnosticsCollector;
use mesh_core_locale::LocaleEngine;
use mesh_core_module::DependencyGraphError;
use mesh_core_module::lifecycle::{ModuleInstance, ModuleState};
use mesh_core_module::package::{
    InstalledModuleGraph, ModuleKind, PackageTransaction, RootModuleGraphManifest, ShellProfile,
};
#[cfg(test)]
use mesh_core_service::InterfaceProvider;
use mesh_core_service::{
    InterfaceRegistry, canonical_interface_name, canonical_interface_name_cow,
    canonical_interface_name_owned,
};
use mesh_core_theme::ThemeEngine;
use mesh_core_wayland::{ClipboardWriter, Layer, StubSurface, WaylandClipboard};

use std::collections::{HashMap, VecDeque};
use std::env;
use std::io::Read;
use std::os::fd::{AsFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use tokio::runtime::Runtime;
use tokio::sync::mpsc;
use tokio::task::{AbortHandle, JoinHandle};

mod backend;
mod component;
mod core_provider;
mod discovery;
mod file_watch;
mod ipc;
mod module_config;
mod package;
mod profile;
mod runtime;
mod service;
mod sounds;
mod surface_layout;
mod types;

#[cfg(test)]
use backend::{BackendLaunchCandidate, backend_launch_candidates_from_graph};
use backend::{BackendRuntimeStatus, BackendRuntimeStatusEntry};
use core_provider::CoreServiceRegistry;
use ipc::spawn_ipc_server;
use mesh_core_backend::BackendServiceEvent;
use mesh_core_presentation::{
    PresentationEngine, WindowEvent, WindowKeyEvent, coalesce_input_events,
};
use mesh_core_render::{DebugOverlay, PixelBuffer};
use runtime::EffectScheduler;
use sounds::{SoundKind, shell_sound_request};
use surface_layout::{
    apply_font_family, default_surface_visibility, load_active_theme, prepare_theme_for_graph,
    selected_theme_mode,
};
use types::{
    BackendIdentity, CommandThrottleState, CompiledContractField, ComponentRuntime,
    ContractValidationCache, IpcProfileSwitchResponse, LatestServiceState,
    PendingBoundServiceState, PendingServiceCommand, ServiceCallRoute, ServiceCommandMsg,
    ServiceDeliveryIndex, SettingsWatchState, ShellCoreState, ShellMessage, SurfaceState,
    TargetRef, ThemeWatchState,
};

/// An owned duplicate of the shell wake descriptor. Workers keep this handle
/// in their own task/thread state instead of borrowing the shell's raw fd, so
/// a late wake can never target a closed or reused descriptor.
#[derive(Debug, Clone)]
pub(in crate::shell) struct WakeHandle {
    fd: Arc<OwnedFd>,
}

impl WakeHandle {
    pub(in crate::shell) fn from_fd(fd: &OwnedFd) -> std::io::Result<Self> {
        Ok(Self {
            fd: Arc::new(fd.try_clone()?),
        })
    }

    pub(in crate::shell) fn wake(&self) {
        let _ = rustix::io::write(self.fd.as_fd(), &1u64.to_ne_bytes());
    }
}
pub use profile::{ActiveSnapshot, CandidatePreview, CandidatePreviewSurface};
pub use types::{
    ChildSurfaceDiagnostic, ComponentContext, ComponentError, ComponentInput, CoreEvent,
    CoreRequest, FrontendEffectRevision, FrontendFrame, FrontendFrameEffects, FrontendFrameError,
    FrontendFrameRevision, FrontendFrameRevisions, FrontendInvalidation, FrontendPaintMetadata,
    FrontendServiceSnapshot, KeyModifiers, PopoverSurfaceRelationship, PopoverTriggerReference,
    ServiceEvent, ServiceInterfaceEventSubscription, ServiceObservationSummary, ShellComponent,
    SurfaceExtent, SurfaceId, TabFocusTarget,
};

/// Ordered lifecycle phases for shutting down the shell. Quiescing is the
/// admission boundary: once entered, new external work is rejected while
/// already accepted component teardown is allowed to settle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ShellShutdownPhase {
    Running,
    Quiescing,
    StoppingComponents,
    StoppingProviders,
    DestroyingPresentation,
    StoppingWorkers,
    Flushing,
    Stopped,
}

impl ShellShutdownPhase {
    pub const fn accepts_external_work(self) -> bool {
        matches!(self, Self::Running)
    }

    pub const fn is_stopped(self) -> bool {
        matches!(self, Self::Stopped)
    }
}

use service::{service_capabilities, service_name_from_interface};

/// The durable revision that identifies one effective control-plane snapshot.
/// Shared settings and profile settings are separate files, so the revision
/// carries both counters instead of manufacturing a content hash that cannot
/// be checked against either persistence boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(in crate::shell) struct DurableControlPlaneRevision {
    pub(in crate::shell) shared: u64,
    pub(in crate::shell) profile: Option<u64>,
}

impl DurableControlPlaneRevision {
    pub(in crate::shell) fn new(shared: u64, profile: Option<u64>) -> Self {
        Self { shared, profile }
    }

    pub(in crate::shell) fn as_string(self) -> String {
        match self.profile {
            Some(profile) => format!("shared:{};profile:{profile}", self.shared),
            None => format!("shared:{}", self.shared),
        }
    }
}

/// Identifies which persisted composition policy owns shell activation.
///
/// The absence of `active-profile` is an intentional migration-era legacy
/// mode. It must not be represented by the same value as a malformed graph or
/// profile, because the latter must fail closed into recovery rather than
/// enabling an implicit set of frontends or backends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::shell) enum ShellCompositionMode {
    LegacyNoProfile,
    ConfiguredProfile { id: String },
    Recovery { reason: String },
}

impl ShellCompositionMode {
    pub(in crate::shell) fn is_recovery(&self) -> bool {
        matches!(self, Self::Recovery { .. })
    }

    pub(in crate::shell) fn service_name(&self) -> &'static str {
        match self {
            Self::LegacyNoProfile => "legacy_no_profile",
            Self::ConfiguredProfile { .. } => "configured_profile",
            Self::Recovery { .. } => "recovery",
        }
    }

    pub(in crate::shell) fn recovery_reason(&self) -> Option<&str> {
        match self {
            Self::Recovery { reason } => Some(reason),
            Self::LegacyNoProfile | Self::ConfiguredProfile { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct PendingPopupGrab {
    identity: mesh_core_presentation::PointerButtonIdentity,
    dispatch_generation: u64,
}

/// Translates the user's blur settings into the painter's quality knobs.
/// Values outside the painter's supported range are clamped rather than
/// rejected: a settings file asking for eight passes gets the most the painter
/// offers, not an unpainted blur.
fn blur_quality_from_settings(
    settings: &mesh_core_config::BlurSettings,
) -> mesh_core_render::BlurQuality {
    mesh_core_render::BlurQuality {
        passes: settings.passes.clamp(1, mesh_core_render::MAX_BLUR_PASSES),
        max_radius: settings.max_radius.max(0.0),
    }
}

pub(crate) fn prepare_icon_pack_bindings(
    module_id: &str,
    module_dir: &Path,
    section: &mesh_core_module::manifest::IconPackSection,
) -> Result<mesh_core_icon::IconPackBindings, String> {
    prepare_icon_pack_bindings_with_cancellation(
        module_id,
        module_dir,
        section,
        &mesh_core_resources::ResourcePreparationToken::new(),
    )
}

pub(crate) fn prepare_icon_pack_bindings_with_cancellation(
    module_id: &str,
    module_dir: &Path,
    section: &mesh_core_module::manifest::IconPackSection,
    cancellation: &mesh_core_resources::ResourcePreparationToken,
) -> Result<mesh_core_icon::IconPackBindings, String> {
    if cancellation.is_cancelled() {
        return Err("resource preparation cancelled".into());
    }
    if section.id.trim().is_empty() {
        tracing::warn!(
            "module {} declares mesh.contributes.icons but the icon-pack id is empty; skipping",
            module_id
        );
        return Err(format!(
            "module {} declares mesh.contributes.icons with an empty icon-pack id",
            module_id
        ));
    }
    let mut font_aliases = std::collections::HashMap::new();
    let mut seen_aliases = std::collections::HashSet::new();
    for req in &section.requires.fonts {
        if cancellation.is_cancelled() {
            return Err("resource preparation cancelled".into());
        }
        if req.alias.trim().is_empty() {
            return Err(format!(
                "icon-pack '{}' declares a font with an empty alias",
                module_id
            ));
        }
        if req.family.trim().is_empty() {
            return Err(format!(
                "icon-pack '{}' declares font alias '{}' with an empty family",
                module_id, req.alias
            ));
        }
        if !seen_aliases.insert(req.alias.clone()) {
            return Err(format!(
                "icon-pack '{}' declares duplicate font alias '{}'",
                module_id, req.alias
            ));
        }

        let (glyph_map_path, prepared_glyphs) = match req.glyph_map.as_deref() {
            Some(path) => {
                let (glyph_map_path, bytes) = read_module_resource_with_cancellation(
                    module_id,
                    module_dir,
                    path,
                    "glyph map",
                    mesh_core_icon::MAX_GLYPH_MAP_BYTES,
                    cancellation,
                )?;
                let glyphs =
                    mesh_core_icon::parse_glyph_map_bytes_with_cancellation(&bytes, cancellation)
                        .map_err(|error| {
                        format!(
                            "icon-pack '{}' font alias '{}' has an invalid glyph map '{}': {error}",
                            module_id, req.alias, path
                        )
                    })?;
                (Some(glyph_map_path), Some(std::sync::Arc::new(glyphs)))
            }
            None => (None, None),
        };

        let (resolved_font_path, prepared_font, font_fingerprint) = if let Some(path) =
            req.file.as_deref()
        {
            let (font_path, bytes) = read_module_resource_with_cancellation(
                module_id,
                module_dir,
                path,
                "font file",
                mesh_core_resources::DEFAULT_MAX_RESOURCE_BYTES,
                cancellation,
            )?;
            mesh_core_icon::validate_font_bytes(&bytes).map_err(|error| {
                format!(
                    "icon-pack '{}' font alias '{}' has an invalid font '{}': {error}",
                    module_id, req.alias, path
                )
            })?;
            let font_fingerprint = mesh_core_resources::resource_fingerprint(&font_path);
            (
                Some(font_path),
                Some(std::sync::Arc::from(bytes)),
                font_fingerprint,
            )
        } else {
            let path = mesh_core_resources::system_resource_catalog()
                .font_path_for_family(&req.family)
                .ok_or_else(|| {
                format!(
                    "icon-pack '{}' font alias '{}' requires installed font family '{}', but it was not found",
                    module_id, req.alias, req.family
                )
            })?;
            let bytes =
                read_host_resource_with_cancellation(&path, cancellation).map_err(|error| {
                    format!(
                        "icon-pack '{}' font alias '{}' cannot read resolved font '{}': {error}",
                        module_id,
                        req.alias,
                        path.display()
                    )
                })?;
            mesh_core_icon::validate_font_bytes(&bytes).map_err(|error| {
                format!(
                    "icon-pack '{}' font alias '{}' has an invalid resolved font '{}': {error}",
                    module_id,
                    req.alias,
                    path.display()
                )
            })?;
            let font_fingerprint = mesh_core_resources::resource_fingerprint(&path);
            (
                Some(path),
                Some(std::sync::Arc::from(bytes)),
                font_fingerprint,
            )
        };

        font_aliases.insert(
            req.alias.clone(),
            mesh_core_icon::FontAsset {
                family: req.family.clone(),
                glyph_map_path,
                resolved_font_path,
                prepared_font,
                font_fingerprint,
                prepared_glyphs,
            },
        );
    }

    for (name, mapping) in &section.mappings {
        if name.trim().is_empty() || mapping.target.trim().is_empty() {
            return Err(format!(
                "icon-pack '{}' declares an icon mapping with an empty name or target",
                module_id
            ));
        }
        if cancellation.is_cancelled() {
            return Err("resource preparation cancelled".into());
        }
        if std::path::Path::new(&mapping.target).is_absolute()
            || mapping.target.trim_start().starts_with("~/")
        {
            return Err(format!(
                "icon-pack '{}' mapping '{}' uses an absolute or home-relative target",
                module_id, name
            ));
        }
        let (alias, glyph_name) =
            mesh_core_icon::parse_target(&mapping.target).ok_or_else(|| {
                format!(
                    "icon-pack '{}' mapping '{}' has malformed target '{}'; expected pack/name",
                    module_id, name, mapping.target
                )
            })?;
        if let Some(font) = font_aliases.get(alias) {
            let glyphs = font.prepared_glyphs.as_ref().ok_or_else(|| {
                format!(
                    "icon-pack '{}' mapping '{}' references font alias '{}' without a glyph map",
                    module_id, name, alias
                )
            })?;
            if !glyphs.contains_key(glyph_name) {
                return Err(format!(
                    "icon-pack '{}' mapping '{}' references missing glyph '{}' in font alias '{}'",
                    module_id, name, glyph_name, alias
                ));
            }
        }
    }
    let axes = mesh_core_icon::SupportedAxes {
        fill: section.axes.fill,
        weight: section.axes.weight,
        grade: section.axes.grade,
        optical_size: section.axes.optical_size,
    };
    let bindings = mesh_core_icon::IconPackBindings {
        pack_id: section.id.clone(),
        module_id: module_id.to_string(),
        mappings: section
            .mappings
            .iter()
            .map(|(name, mapping)| {
                (
                    name.clone(),
                    mesh_core_icon::IconMapping {
                        target: mapping.target.clone(),
                        multicolor: mapping.multicolor,
                    },
                )
            })
            .collect(),
        axes,
        font_aliases,
    };
    if cancellation.is_cancelled() {
        return Err("resource preparation cancelled".into());
    }
    tracing::debug!(
        "prepared icon-pack '{}' (id={}, mappings={}, font_aliases={})",
        module_id,
        section.id,
        section.mappings.len(),
        section.requires.fonts.len()
    );
    Ok(bindings)
}

fn read_module_resource_with_cancellation(
    module_id: &str,
    module_dir: &Path,
    declared: &str,
    label: &str,
    max_bytes: usize,
    cancellation: &mesh_core_resources::ResourcePreparationToken,
) -> Result<(PathBuf, Vec<u8>), String> {
    let handle =
        mesh_core_resources::ResourceAssetHandle::new(module_dir, declared).map_err(|error| {
            format!("module {module_id} declares unsafe {label} '{declared}': {error}")
        })?;
    let bytes = handle
        .read_bounded_with_cancellation(max_bytes, cancellation)
        .map_err(|error| {
            format!("module {module_id} declares unreadable {label} '{declared}': {error}")
        })?;
    Ok((handle.candidate_path(), bytes))
}

fn read_host_resource_with_cancellation(
    path: &Path,
    cancellation: &mesh_core_resources::ResourcePreparationToken,
) -> Result<Vec<u8>, String> {
    let mut file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.file_type().is_file() {
        return Err("resource is not a regular file".into());
    }
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 64 * 1024];
    let limit = mesh_core_resources::DEFAULT_MAX_RESOURCE_BYTES.saturating_add(1);
    while bytes.len() < limit {
        if cancellation.is_cancelled() {
            return Err("resource preparation cancelled".into());
        }
        let amount = (limit - bytes.len()).min(chunk.len());
        let read = file
            .read(&mut chunk[..amount])
            .map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..read]);
    }
    if cancellation.is_cancelled() {
        return Err("resource preparation cancelled".into());
    }
    if bytes.len() > mesh_core_resources::DEFAULT_MAX_RESOURCE_BYTES {
        return Err(format!(
            "resource exceeds {} bytes",
            mesh_core_resources::DEFAULT_MAX_RESOURCE_BYTES
        ));
    }
    Ok(bytes)
}

fn shell_global_shortcut_request(
    key: &str,
    ctrl: bool,
    shift: bool,
    debug_enabled: bool,
) -> Option<CoreRequest> {
    match key.to_ascii_lowercase().as_str() {
        "d" if ctrl && shift => Some(CoreRequest::ToggleDebugOverlay),
        "c" if ctrl && shift => Some(CoreRequest::ToggleDebugElementPicker),
        "tab" | "iso_left_tab" if ctrl && debug_enabled => Some(CoreRequest::CycleDebugTab),
        _ => None,
    }
}

fn component_key_pressed_input(key: String, ctrl: bool, shift: bool, alt: bool) -> ComponentInput {
    ComponentInput::KeyPressed {
        key,
        modifiers: KeyModifiers { ctrl, shift, alt },
    }
}

fn component_key_released_input(key: String, modifiers: KeyModifiers) -> ComponentInput {
    ComponentInput::KeyReleased { key, modifiers }
}

fn update_modifiers_for_key_release(modifiers: &mut KeyModifiers, key: &str) {
    let normalized = key.to_ascii_lowercase();
    if normalized.contains("shift") {
        modifiers.shift = false;
    } else if normalized.contains("control") || normalized == "ctrl" {
        modifiers.ctrl = false;
    } else if normalized.contains("alt") {
        modifiers.alt = false;
    }
}

pub struct Shell {
    pub config: ShellConfig,
    /// Core shell preferences, resolved from [`Self::settings_store`]. Kept
    /// alongside the store because most readers want only this.
    pub settings: ShellSettings,
    /// Every user decision in the shell, in one document. Shared with each
    /// component so they read the same snapshot the shell does.
    pub settings_store: Arc<SettingsStore>,
    /// Revision of the exact shared/profile settings snapshot currently
    /// visible to the shell and all mounted components.
    control_plane_revision: DurableControlPlaneRevision,
    pub theme: ThemeEngine,
    pub locale: LocaleEngine,
    pub diagnostics: DiagnosticsCollector,
    pub interfaces: InterfaceRegistry,
    /// Host-backed service providers use the same identity and command seam
    /// as module-backed providers, even when their final action stays in the
    /// shell process.
    core_service_providers: CoreServiceRegistry,
    /// Immutable core-owned interface contracts and providers. Graph
    /// activation rebuilds its catalog from this baseline so a replacement
    /// cannot retain entries from an older graph generation.
    builtin_interface_catalog: mesh_core_service::InterfaceCatalog,
    /// Catalog-backed policy loaded from the root graph's persisted approvals.
    capability_policy: CapabilityPolicy,
    /// Effective grants for the current activation candidate. Runtime
    /// construction must consume this map rather than manifest declarations.
    effective_capabilities: Arc<HashMap<String, EffectiveCapabilities>>,
    /// Graph portion of the canonical authoring snapshot currently committed
    /// to runtime. Its revision remains available to runtime observers while
    /// the shell keeps its prepared runtime mirrors alongside it.
    installed_module_graph: Option<InstalledModuleGraph>,
    resource_snapshot: Arc<discovery::ResourceSnapshot>,
    resource_explanation: Arc<mesh_core_resources::ResourceExplanationSnapshot>,
    pub font_registry: mesh_core_resources::FontRegistry,
    font_renderer_revision: u64,
    resource_preparation: mesh_core_resources::ResourcePreparationCoordinator,
    composition_mode: ShellCompositionMode,
    active_profile_id: Option<String>,
    modules: HashMap<String, ModuleInstance>,
    frontend_catalog: component::FrontendCatalogHandle,
    module_dirs: Vec<PathBuf>,
    core: ShellCoreState,
    /// Last rendered snapshot sent through the authoritative theme interface.
    /// It is the baseline for deterministic revisioned token events.
    last_published_theme_snapshot: Option<mesh_core_theme::ThemeSnapshot>,
    components: Vec<ComponentRuntime>,
    components_want_render: bool,
    /// True after a component presented; false after a render pass with zero
    /// presents.  When false, `components_have_ready_render_work` is suppressed
    /// so stale `wants_render()` flags cannot spin the idle loop.
    presented_last_frame: bool,
    component_by_surface: HashMap<SurfaceId, usize>,
    service_delivery_index: ServiceDeliveryIndex,
    surfaces: HashMap<SurfaceId, StubSurface>,
    clipboard: Box<dyn ClipboardWriter>,
    presentation_engine: PresentationEngine,
    ipc_server: Option<ipc::IpcServerHandle>,
    file_watcher: Option<file_watch::FileWatcherHandle>,
    file_watcher_tx: Option<mpsc::UnboundedSender<ShellMessage>>,
    file_watch_set: file_watch::WatchSet,
    theme_watch: ThemeWatchState,
    settings_watch: SettingsWatchState,
    next_theme_reload_check: std::time::Instant,
    next_shell_settings_reload_check: std::time::Instant,
    next_frontend_reload_check: std::time::Instant,
    file_watcher_active: bool,
    debug: DebugOverlayState,
    debug_overlay: DebugOverlay,
    active_key_modifiers: KeyModifiers,
    keyboard_focus_surface: Option<SurfaceId>,
    pending_wayland_events: VecDeque<WindowEvent>,
    /// Click credentials captured from the current Wayland dispatch. Entries
    /// are consumed when a popup is created and never reused for repositioning
    /// or a later dispatch generation.
    pending_popup_grabs: HashMap<String, PendingPopupGrab>,
    popup_grab_generation: u64,
    transfer_owned_keyboard_modes: HashMap<SurfaceId, mesh_core_wayland::KeyboardMode>,
    service_handlers: HashMap<String, mpsc::Sender<ServiceCommandMsg>>,
    backend_runtimes: HashMap<String, BackendRuntimeSlot>,
    pending_backend_runtimes: HashMap<String, PendingBackendRuntime>,
    pending_resource_preparation: Option<profile::PendingResourcePreparation>,
    pending_profile_switch: Option<profile::PendingProfileSwitch>,
    /// Prepared activation surfaces remain isolated from the live presentation
    /// maps until their candidate becomes the active snapshot.
    candidate_preview: Option<Arc<profile::CandidatePreview>>,
    /// Monotonic identity of the last committed activation plan.
    activation_generation: u64,
    /// The immutable runtime snapshot swapped at activation commit.
    active_snapshot: Option<Arc<profile::ActiveSnapshot>>,
    /// Monotonic provider epochs prevent delayed output from an older runtime
    /// for the same provider id from crossing a replacement boundary.
    backend_provider_epochs: HashMap<String, u64>,
    effect_scheduler: EffectScheduler,
    backend_runtime_statuses: BackendRuntimeStatusMap,
    /// The latest provider generation that crossed an activation commit. A
    /// candidate may have an identity before commit, but it must not publish
    /// availability until it is recorded here.
    committed_provider_generations: HashMap<String, CommittedProviderGeneration>,
    backend_supervision: HashMap<String, backend::BackendSupervisionState>,
    backend_respawn: Option<backend::BackendRespawnContext>,
    retiring_backend_runtimes: Vec<BackendRuntimeTasks>,
    backend_restart_tasks: Vec<JoinHandle<()>>,
    shutdown_phase: ShellShutdownPhase,
    /// Shutdown broadcasts/unmounts are internal bounded work and remain
    /// admissible while external requests are rejected after quiescing.
    shutdown_effects_allowed: bool,
    shutdown_started: bool,
    shutdown_complete: bool,
    latest_service_state: HashMap<String, LatestServiceState>,
    /// Last committed provider health event, replayed when a consumer mounts
    /// after a graph/runtime transition.
    latest_service_health: HashMap<String, ServiceEvent>,
    /// Identity for the cached health event; kept separate from the public
    /// event payload so generation metadata does not leak into service state.
    latest_service_health_identities: HashMap<String, BackendIdentity>,
    service_contract_validation: HashMap<String, ContractValidationCache>,
    /// Command-bound service state awaiting provider confirmation, keyed by
    /// (interface, state field). Replacing an entry makes the newer CallId the
    /// sole owner of rollback for that field.
    pending_bound_service_state: HashMap<(String, String), PendingBoundServiceState>,
    /// Historical owners let a newer failed write reveal an older still-live
    /// write, or skip over older writes that already failed.
    bound_service_state_transactions: HashMap<mesh_core_backend::CallId, PendingBoundServiceState>,
    command_throttle: HashMap<(String, String, String), CommandThrottleState>,
    pending_service_call_routes: HashMap<u64, ServiceCallRoute>,
    pending_popover_hides: HashMap<SurfaceId, std::time::Instant>,
    profiling: runtime::profiling::ProfilingRuntimeState,
    wake_handle: Option<WakeHandle>,
    /// Kept after every lifecycle guard so the shared wake handle is released
    /// only after workers have been stopped or joined, including `Drop`.
    eventfd_fd: Option<OwnedFd>,
}

impl Drop for Shell {
    fn drop(&mut self) {
        self.unmount_components();
        self.backend_respawn = None;
        for task in self.backend_restart_tasks.drain(..) {
            task.abort();
        }
        self.service_handlers.clear();
        self.backend_runtimes.clear();
        self.pending_backend_runtimes.clear();
        self.candidate_preview = None;
        self.pending_profile_switch = None;
        self.retiring_backend_runtimes.clear();
        self.ipc_server.take();
        self.file_watcher_tx = None;
        if let Some(file_watcher) = self.file_watcher.take() {
            file_watcher.stop_and_join();
        }
        self.wake_handle.take();
        self.eventfd_fd.take();
        self.shutdown_phase = ShellShutdownPhase::Stopped;
        self.shutdown_started = true;
        self.shutdown_complete = true;
    }
}

impl Shell {
    /// Return the last coherent runtime activation. The snapshot is swapped
    /// only after candidate resources, providers, components, and control
    /// plane state have crossed the activation commit boundary.
    pub fn active_snapshot(&self) -> Option<Arc<ActiveSnapshot>> {
        self.active_snapshot.clone()
    }

    /// Return the currently prepared activation candidate, if any. Candidate
    /// surfaces are always hidden and remain outside the live presentation
    /// maps until the candidate commits.
    pub fn candidate_preview(&self) -> Option<Arc<CandidatePreview>> {
        self.candidate_preview.clone()
    }

    pub fn shutdown_phase(&self) -> ShellShutdownPhase {
        self.shutdown_phase
    }

    pub(in crate::shell) fn begin_shutdown(&mut self) -> bool {
        if !self.shutdown_phase.accepts_external_work() {
            return false;
        }
        self.shutdown_phase = ShellShutdownPhase::Quiescing;
        self.shutdown_started = true;
        self.core.shutting_down = true;
        tracing::info!(phase = ?self.shutdown_phase, "shell entered shutdown quiescing");
        true
    }

    pub(in crate::shell) fn advance_shutdown_phase(&mut self, phase: ShellShutdownPhase) {
        if self.shutdown_phase == phase || self.shutdown_phase.is_stopped() {
            return;
        }
        debug_assert!(phase > self.shutdown_phase);
        self.shutdown_phase = phase;
        tracing::debug!(phase = ?self.shutdown_phase, "shell shutdown phase advanced");
    }

    pub(in crate::shell) fn clear_candidate_preview(&mut self, generation: u64) {
        if self
            .candidate_preview
            .as_ref()
            .is_some_and(|preview| preview.generation() == generation)
        {
            self.candidate_preview = None;
        }
    }

    pub(in crate::shell) fn mark_candidate_backend_ready(&mut self, interface: &str) {
        let Some(preview) = self.candidate_preview.as_ref() else {
            return;
        };
        self.candidate_preview = Some(Arc::new(preview.with_backend_ready(interface)));
    }

    /// Return the live trigger-to-popup relationships retained by promoted
    /// popovers. The relationship is deliberately exposed at the shell
    /// boundary so focus, dismissal, accessibility, and debug integrations
    /// can observe the same identity after promotion instead of reconstructing
    /// it from compositor parentage.
    pub fn popover_surface_relationships(&self) -> Vec<PopoverSurfaceRelationship> {
        let mut relationships = self
            .components
            .iter()
            .flat_map(|runtime| {
                runtime
                    .targets()
                    .filter_map(|target| target.popover_relationship.clone())
            })
            .collect::<Vec<_>>();
        relationships.sort_by(|left, right| {
            left.popup_surface_id
                .cmp(&right.popup_surface_id)
                .then_with(|| left.trigger_surface_id.cmp(&right.trigger_surface_id))
                .then_with(|| left.popup_node_key.cmp(&right.popup_node_key))
        });
        relationships
    }
}

type BackendRuntimeStatusMap = HashMap<String, HashMap<String, BackendRuntimeStatusEntry>>;

#[derive(Debug, Clone, PartialEq, Eq)]
struct CommittedProviderGeneration {
    provider_id: String,
    identity: BackendIdentity,
    retired: bool,
}

#[derive(Debug, Clone)]
struct BackendRuntimeSlot {
    interface: String,
    provider_id: String,
    /// Identity carried by events from this particular runtime generation.
    /// Candidate generations use a private value until they are committed.
    event_provider_id: Arc<std::sync::RwLock<String>>,
    identity: Arc<RwLock<mesh_core_backend::BackendIdentity>>,
    generation: u64,
    command_tx: mpsc::Sender<ServiceCommandMsg>,
    task: AbortHandle,
    tasks: Option<BackendRuntimeTasks>,
}

/// Retained handles for the service and event bridge tasks belonging to one
/// backend. Dropping a command sender requests the authored stop hook; these
/// handles keep the shell responsible for joining both tasks afterwards.
#[derive(Debug, Clone)]
struct BackendRuntimeTasks {
    service: Arc<Mutex<Option<JoinHandle<()>>>>,
    bridge: Arc<Mutex<Option<JoinHandle<()>>>>,
}

impl BackendRuntimeTasks {
    fn new(service: JoinHandle<()>, bridge: JoinHandle<()>) -> Self {
        Self {
            service: Arc::new(Mutex::new(Some(service))),
            bridge: Arc::new(Mutex::new(Some(bridge))),
        }
    }

    fn abort(&self) {
        if let Some(task) = self.service.lock().unwrap().as_ref() {
            task.abort();
        }
        if let Some(task) = self.bridge.lock().unwrap().as_ref() {
            task.abort();
        }
    }

    fn take_service(&self) -> Option<JoinHandle<()>> {
        self.service.lock().unwrap().take()
    }

    fn take_bridge(&self) -> Option<JoinHandle<()>> {
        self.bridge.lock().unwrap().take()
    }
}

impl Drop for BackendRuntimeTasks {
    fn drop(&mut self) {
        self.abort();
    }
}

#[derive(Debug, Clone)]
struct PendingBackendRuntime {
    slot: BackendRuntimeSlot,
    graph_path: Option<PathBuf>,
    started: bool,
    initial_state: Option<serde_json::Value>,
}

pub fn default_ipc_socket_path() -> PathBuf {
    if let Ok(path) = std::env::var("MESH_IPC_SOCKET") {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }

    if let Some(runtime_dir) = non_empty_env_path(std::env::var_os("XDG_RUNTIME_DIR")) {
        return runtime_dir.join("mesh.sock");
    }

    let uid = std::env::var("UID").unwrap_or_else(|_| "unknown".to_string());
    PathBuf::from("/tmp")
        .join(format!("mesh-{uid}"))
        .join("mesh.sock")
}

fn non_empty_env_path(value: Option<std::ffi::OsString>) -> Option<PathBuf> {
    value.filter(|value| !value.is_empty()).map(PathBuf::from)
}

impl Default for Shell {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ShellRunError {
    #[error("failed to initialize async runtime: {0}")]
    RuntimeInit(std::io::Error),

    #[error("eventfd creation failed: {0}")]
    EventfdCreate(String),

    #[error(transparent)]
    Component(#[from] ComponentError),

    #[error("failed to compile frontend module '{module_id}': {source}")]
    FrontendCompile {
        module_id: String,
        source: mesh_core_frontend::CompileFrontendError,
    },

    #[error(transparent)]
    DependencyGraph(#[from] DependencyGraphError),

    #[error(transparent)]
    CapabilityPolicy(#[from] mesh_core_capability::CapabilityPolicyError),

    #[error(transparent)]
    ModuleGraph(#[from] mesh_core_module::package::ModuleManifestError),

    #[error("locale catalog preparation failed: {0}")]
    LocaleCatalog(String),

    #[error("{message}")]
    FrontendComposition { message: String },

    #[error("missing shell surface: {0}")]
    MissingSurface(String),

    #[error(transparent)]
    Presentation(#[from] mesh_core_presentation::PresentationError),

    #[error("failed to initialize ipc socket at {path}: {source}")]
    IpcInit {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("package operation failed: {0}")]
    Package(String),

    #[error(transparent)]
    Theme(#[from] mesh_core_theme::ThemeError),

    #[error(
        "buffer allocation rejected for {requested_bytes} bytes (max {max_bytes}); surface '{surface_id}' at scale {scale:.2} would require {logical_w}x{logical_h} logical -> {physical_w}x{physical_h} physical"
    )]
    BufferAlloc {
        surface_id: String,
        logical_w: u32,
        logical_h: u32,
        physical_w: u32,
        physical_h: u32,
        scale: f32,
        requested_bytes: u64,
        max_bytes: u64,
    },
}

fn resolve_default_module_dirs(config: &ShellConfig) -> Vec<PathBuf> {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    resolve_discovery_paths(&workspace_root, &config.shell.discovery_paths)
}

#[cfg(test)]
mod tests;
