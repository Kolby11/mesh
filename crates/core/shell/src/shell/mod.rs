use mesh_core_config::{
    ShellConfig, ShellSettings, default_settings_path, load_config, load_shell_settings,
    resolve_discovery_paths,
};
use mesh_core_debug::{
    BackendRuntimeEntry, DebugOverlayState, DebugSnapshot, HealthEntry, InterfaceEntry,
    ModuleEntry, ProviderEntry,
};
use mesh_core_diagnostics::DiagnosticsCollector;
use mesh_core_events::EventBus;
use mesh_core_locale::LocaleEngine;
use mesh_core_module::lifecycle::{ModuleInstance, ModuleState};
use mesh_core_module::package::{InstalledModuleGraph, ModuleKind, load_installed_module_graph};
use mesh_core_module::{DependencyGraphError, ModuleType, validate_module_dependency_graph};
use mesh_core_service::{
    InterfaceContract, InterfaceProvider, InterfaceRegistry, ServiceRegistry,
    canonical_interface_name, load_interface_contract,
};
use mesh_core_theme::ThemeEngine;
use mesh_core_wayland::{ClipboardWriter, Layer, StubSurface, WaylandClipboard};

use std::collections::{HashMap, VecDeque};
use std::env;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::runtime::Runtime;
use tokio::sync::mpsc;
use tokio::task::AbortHandle;

mod backend;
mod component;
mod discovery;
mod ipc;
mod runtime;
mod service;
mod sounds;
mod surface_layout;
mod types;

#[cfg(test)]
use backend::{BackendLaunchCandidate, backend_launch_candidates_from_graph};
use backend::{BackendRuntimeStatus, BackendRuntimeStatusEntry};
use ipc::spawn_ipc_server;
use mesh_core_backend::{BackendServiceEvent, spawn_backend_service};
use mesh_core_presentation::{
    LayerSurfaceConfig, PresentationEngine, WindowEvent, WindowKeyEvent, coalesce_pointer_moves,
    event_surface_id,
};
use mesh_core_render::{DebugOverlay, PixelBuffer};
use sounds::{SoundKind, play_shell_sound};
use surface_layout::{default_surface_visibility, load_active_theme};
use types::{
    CommandThrottleState, ComponentRuntime, LatestServiceState, ServiceCommandMsg,
    SettingsWatchState, ShellCoreState, ShellMessage, SurfaceState, ThemeWatchState,
};
pub use types::{
    ComponentContext, ComponentError, ComponentInput, CoreEvent, CoreRequest, KeyModifiers,
    ServiceEvent, ShellComponent, SurfaceId, TabFocusTarget,
};

use service::{service_command_control_capability, service_name_from_interface};

/// If `assets.icons` is declared in a module manifest, register an icon pack
/// at `<module_id>` rooted at `<module_dir>/<assets.icons.path>`. The pack
/// kind comes from the manifest — XDG directory layout by default, or a
/// font-glyph pack when `kind = "font"`. Authors can then reference module
/// assets via candidates like `<module_id>:<name>` in `icons.toml`. No-op
/// when the manifest doesn't ship icons. Failures log but don't abort
/// module discovery — a module with bad icons should still load.
fn register_module_icon_pack(
    module_id: &str,
    module_dir: &Path,
    assets: Option<&mesh_core_module::manifest::AssetsSection>,
) {
    use mesh_core_module::manifest::{IconAssets, IconAssetsKind};

    let Some(icons) = assets.and_then(|a| a.icons.as_ref()) else {
        return;
    };
    let root = module_dir.join(icons.path());
    if !root.is_dir() {
        tracing::warn!(
            "module {} declares assets.icons={} but {} is not a directory; skipping pack",
            module_id,
            icons.path(),
            root.display()
        );
        return;
    }

    let kind = match icons {
        IconAssets::Path(_) => mesh_core_icon::IconPackKind::Xdg,
        IconAssets::Detailed(detailed) => match detailed.kind {
            IconAssetsKind::Xdg => mesh_core_icon::IconPackKind::Xdg,
            IconAssetsKind::Font => {
                let Some(font_file) = detailed.font_file.clone() else {
                    tracing::warn!(
                        "module {} declares font icon pack at {} but is missing assets.icons.font_file; skipping",
                        module_id,
                        root.display()
                    );
                    return;
                };
                let Some(glyph_map) = detailed.glyph_map.clone() else {
                    tracing::warn!(
                        "module {} declares font icon pack at {} but is missing assets.icons.glyph_map; skipping",
                        module_id,
                        root.display()
                    );
                    return;
                };
                mesh_core_icon::IconPackKind::Font {
                    font_file,
                    glyph_map,
                }
            }
        },
    };

    let pack = mesh_core_icon::IconPackRoot {
        id: module_id.to_string(),
        root: Some(root.clone()),
        theme: "hicolor".into(),
        kind,
    };
    match mesh_core_icon::register_default_pack(pack) {
        Ok(true) => tracing::info!(
            "registered icon pack '{}' from {}",
            module_id,
            root.display()
        ),
        Ok(false) => tracing::debug!(
            "icon pack '{}' already registered; leaving existing root in place",
            module_id
        ),
        Err(err) => tracing::warn!(
            "failed to register icon pack '{}' from {}: {err}",
            module_id,
            root.display()
        ),
    }
}

/// Register a frontend module's icon resolution context with the shared
/// icon registry. Combines the frontend's declared `dependencies.icon_packs`
/// + author overrides from the manifest with the user's per-module override
/// of the pack chain and per-icon overrides from shell `settings.json`.
/// The shell-default pack is composed in by the registry itself at lookup
/// time.
fn register_frontend_icon_bindings(
    module_id: &str,
    manifest: &mesh_core_module::manifest::Manifest,
    user_overrides: Option<&mesh_core_config::ModuleIconOverrides>,
) {
    let declared_pack_chain: Vec<String> = manifest
        .dependencies
        .icon_packs
        .required
        .iter()
        .chain(manifest.dependencies.icon_packs.optional.iter())
        .cloned()
        .collect();
    let author_overrides = manifest
        .icons
        .as_ref()
        .map(|i| i.overrides.clone())
        .unwrap_or_default();
    let ignore_default_frontend = manifest
        .icons
        .as_ref()
        .map(|i| i.ignore_shell_default)
        .unwrap_or(false);
    let user_pack_chain = user_overrides.and_then(|u| u.use_packs.clone());
    let user_overrides_map = user_overrides
        .map(|u| u.overrides.clone())
        .unwrap_or_default();
    let ignore_default_user = user_overrides
        .map(|u| u.ignore_shell_default)
        .unwrap_or(false);

    if declared_pack_chain.is_empty()
        && author_overrides.is_empty()
        && user_pack_chain.is_none()
        && user_overrides_map.is_empty()
        && !ignore_default_frontend
        && !ignore_default_user
    {
        return;
    }

    tracing::info!(
        "registered frontend icon bindings for '{}' (chain={:?}, author_overrides={}, user_overrides={})",
        module_id,
        declared_pack_chain,
        author_overrides.len(),
        user_overrides_map.len(),
    );
    mesh_core_icon::set_default_frontend_bindings(
        module_id.to_string(),
        mesh_core_icon::FrontendIconBindings {
            declared_pack_chain,
            author_overrides,
            user_pack_chain,
            user_overrides: user_overrides_map,
            ignore_shell_default_frontend: ignore_default_frontend,
            ignore_shell_default_user: ignore_default_user,
        },
    );
}

/// Register an icon-pack module's binding table with the shared icon
/// registry. Reads `mesh.icon_pack` from the manifest. Soft-warns when
/// declared `requires.fonts` entries can't be matched against any
/// fontconfig family on the system, but still registers the pack so the
/// resolver can produce useful diagnostics for the missing assets.
fn register_icon_pack_module(
    module_id: &str,
    module_dir: &Path,
    icon_pack: Option<&mesh_core_module::manifest::IconPackSection>,
) {
    let Some(section) = icon_pack else { return };
    if section.id.trim().is_empty() {
        tracing::warn!(
            "module {} declares mesh.icon_pack but icon_pack.id is empty; skipping",
            module_id
        );
        return;
    }
    let mut font_aliases = std::collections::HashMap::new();
    for req in &section.requires.fonts {
        if req.alias.trim().is_empty() {
            continue;
        }
        let glyph_map_path = req
            .glyph_map
            .as_deref()
            .map(|p| module_dir.join(p))
            .filter(|p| p.is_file());
        if req.glyph_map.is_some() && glyph_map_path.is_none() {
            tracing::warn!(
                "icon-pack '{}' declares glyph_map for font alias '{}' but file is missing",
                module_id,
                req.alias
            );
        }
        let resolved_font_path = resolve_font_family_path(&req.family);
        if resolved_font_path.is_none() {
            tracing::warn!(
                "icon-pack '{}' requires font family '{}' but it is not installed",
                module_id,
                req.family
            );
        }
        font_aliases.insert(
            req.alias.clone(),
            mesh_core_icon::FontAsset {
                family: req.family.clone(),
                glyph_map_path,
                resolved_font_path,
            },
        );
    }
    let axes = mesh_core_icon::SupportedAxes {
        fill: section.axes.fill,
        weight: section.axes.weight,
        grade: section.axes.grade,
        optical_size: section.axes.optical_size,
    };
    mesh_core_icon::set_default_icon_pack(mesh_core_icon::IconPackBindings {
        pack_id: section.id.clone(),
        module_id: module_id.to_string(),
        mappings: section.mappings.clone(),
        axes,
        font_aliases,
    });
    tracing::info!(
        "registered icon-pack '{}' (id={}, mappings={}, font_aliases={})",
        module_id,
        section.id,
        section.mappings.len(),
        section.requires.fonts.len()
    );
}

/// Resolve a fontconfig family name to its `.ttf`/`.otf` path on disk.
/// Returns `None` when the family is not installed. We shell out to
/// `fc-match -f %{file}` for portability — fontconfig handles family
/// aliases, weights, and synthetic styles for us.
fn resolve_font_family_path(family: &str) -> Option<std::path::PathBuf> {
    let output = std::process::Command::new("fc-match")
        .arg("-f")
        .arg("%{file}")
        .arg(family)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if path.is_empty() {
        return None;
    }
    let pb = std::path::PathBuf::from(path);
    if !pb.is_file() {
        return None;
    }
    // fc-match falls back to *some* font if the requested family isn't
    // installed. Verify the resolved file's basename mentions a sanitized
    // form of the requested family to avoid silent fallback to e.g.
    // DejaVu when Material Symbols isn't installed.
    let needle = family.replace(' ', "").to_lowercase();
    let basename = pb
        .file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_lowercase().replace(' ', ""))
        .unwrap_or_default();
    if !basename.contains(&needle) {
        return None;
    }
    Some(pb)
}

fn shell_global_shortcut_request(
    key: &str,
    ctrl: bool,
    shift: bool,
    debug_enabled: bool,
) -> Option<CoreRequest> {
    match key.to_ascii_lowercase().as_str() {
        "d" if ctrl && shift => Some(CoreRequest::ToggleDebugOverlay),
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
    pub settings: ShellSettings,
    pub theme: ThemeEngine,
    pub locale: LocaleEngine,
    pub events: EventBus,
    pub diagnostics: DiagnosticsCollector,
    pub services: ServiceRegistry,
    pub interfaces: InterfaceRegistry,
    modules: HashMap<String, ModuleInstance>,
    module_dirs: Vec<PathBuf>,
    core: ShellCoreState,
    components: Vec<ComponentRuntime>,
    component_by_surface: HashMap<SurfaceId, usize>,
    surfaces: HashMap<SurfaceId, StubSurface>,
    clipboard: Box<dyn ClipboardWriter>,
    presentation_engine: PresentationEngine,
    theme_watch: ThemeWatchState,
    settings_watch: SettingsWatchState,
    next_theme_reload_check: std::time::Instant,
    next_shell_settings_reload_check: std::time::Instant,
    next_frontend_reload_check: std::time::Instant,
    next_module_settings_reload_check: std::time::Instant,
    debug: DebugOverlayState,
    debug_overlay: DebugOverlay,
    active_key_modifiers: KeyModifiers,
    keyboard_focus_surface: Option<SurfaceId>,
    pending_wayland_events: VecDeque<WindowEvent>,
    transfer_owned_keyboard_modes: HashMap<SurfaceId, mesh_core_wayland::KeyboardMode>,
    service_handlers: HashMap<String, mpsc::UnboundedSender<ServiceCommandMsg>>,
    backend_runtimes: HashMap<String, BackendRuntimeSlot>,
    backend_runtime_statuses: HashMap<(String, String), BackendRuntimeStatusEntry>,
    latest_service_state: HashMap<String, LatestServiceState>,
    pending_audio_muted: Option<bool>,
    command_throttle: HashMap<(String, String), CommandThrottleState>,
    profiling: runtime::profiling::ProfilingRuntimeState,
}

#[derive(Debug, Clone)]
struct BackendRuntimeSlot {
    interface: String,
    provider_id: String,
    command_tx: mpsc::UnboundedSender<ServiceCommandMsg>,
    task: AbortHandle,
}

pub fn default_ipc_socket_path() -> PathBuf {
    if let Ok(path) = std::env::var("MESH_IPC_SOCKET") {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }

    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        return PathBuf::from(runtime_dir).join("mesh.sock");
    }

    let uid = std::env::var("UID").unwrap_or_else(|_| "unknown".to_string());
    PathBuf::from("/tmp")
        .join(format!("mesh-{uid}"))
        .join("mesh.sock")
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

    #[error(transparent)]
    Component(#[from] ComponentError),

    #[error("failed to compile frontend module '{module_id}': {source}")]
    FrontendCompile {
        module_id: String,
        source: mesh_core_frontend::CompileFrontendError,
    },

    #[error(transparent)]
    DependencyGraph(#[from] DependencyGraphError),

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

    #[error(transparent)]
    Theme(#[from] mesh_core_theme::ThemeError),
}

fn resolve_default_module_dirs(config: &ShellConfig) -> Vec<PathBuf> {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    resolve_discovery_paths(&workspace_root, &config.shell.discovery_paths)
}

#[cfg(test)]
mod tests;
