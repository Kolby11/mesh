use super::layout::{
    annotate_overflow_tree, find_click_handler, find_event_handler, find_focusable_at,
    find_node_bounds_by_key, find_node_by_key, find_node_path_at, find_scrollable_at,
    find_tooltip_text_by_key, is_input_key, is_slider_key, measure_content_size,
    namespace_event_handlers, node_tooltip_text, parse_namespaced_handler, scroll_limits,
};
use super::service::{apply_service_update, script_events_to_requests, seed_service_state};
use super::surface_layout::{
    SurfaceLayoutSettings, SurfaceSizePolicy, load_frontend_module_settings,
};
use super::types::{
    ComponentContext, ComponentError, ComponentInput, CoreEvent, CoreRequest, ServiceEvent,
    ShellComponent,
};
mod animation;
mod catalog;
mod composition;
mod diagnostics;
mod input;
mod interaction_state;
mod rendering;
mod runtime;
mod runtime_tree;
mod shell_component;

use animation::StyleAnimation;
pub(in crate::shell) use catalog::FrontendCatalog;
pub(in crate::shell) use runtime_tree::ScrollOffsetState;
use runtime_tree::{
    annotate_runtime_tree, collect_all_keys, collect_element_metrics, input_accepts_char,
};

use mesh_core_capability::{Capability, CapabilitySet};
use mesh_core_diagnostics::Diagnostics;
use mesh_core_elements::{
    LayoutEngine, StyleContext, StyleResolver, VariableStore, WidgetNode, element_snapshot_json,
};
use mesh_core_locale::LocaleEngine;
use mesh_core_render::{
    CompiledFrontendModule, FrontendRenderMode, compile_frontend_module, root_accessibility_role,
};
use mesh_core_scripting::{LocaleBoundState, ScriptContext, ScriptInterfaceImport};
use mesh_core_theme::{Theme, default_theme};
use mesh_core_wayland::{Edge, ShellSurface};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mesh_core_render::{PixelBuffer, SharedTextMeasurer, paint_frontend_tree_at};

const TOOLTIP_DELAY: Duration = Duration::from_millis(500);
const TOOLTIP_OVERLAY_WIDTH: u32 = 260;
const TOOLTIP_OVERLAY_HEIGHT: u32 = 96;

pub(super) struct FrontendSurfaceComponent {
    pub(super) compiled: CompiledFrontendModule,
    pub(super) module_dir: PathBuf,
    module_settings_file: PathBuf,
    settings_json: serde_json::Value,
    pub(super) surface_layout: SurfaceLayoutSettings,
    pub(super) frontend_catalog: FrontendCatalog,
    pub(super) visible: bool,
    dirty: bool,
    last_service_update: Option<String>,
    focused_key: Option<String>,
    pointer_down_key: Option<String>,
    active_slider_key: Option<String>,
    input_values: HashMap<String, String>,
    slider_values: HashMap<String, f32>,
    checked_values: HashMap<String, bool>,
    render_hooks_pending: bool,
    pub(super) scroll_offsets: HashMap<String, ScrollOffsetState>,
    // Hover tracking for CSS :hover and tooltip system.
    hovered_key: Option<String>,
    hovered_path: Vec<String>,
    hovered_pos: (f32, f32),
    hover_start: Option<std::time::Instant>,
    runtimes: Arc<Mutex<HashMap<String, EmbeddedFrontendRuntime>>>,
    render_stack: RefCell<Vec<String>>,
    active_theme: RefCell<Theme>,
    measured_size: Option<(u32, u32)>,
    last_surface_size: Option<(u32, u32)>,
    locale: LocaleEngine,
    interface_catalog: mesh_core_service::InterfaceCatalog,
    last_tree: Option<WidgetNode>,
    diagnostics: Option<Diagnostics>,
    /// Desired visibility for surface portals (`<ImportedSurface hidden={...} />`).
    /// Updated during build_tree; compared to last_surface_states in tick().
    pending_surface_states: RefCell<HashMap<String, bool>>,
    /// Last visibility state emitted for each surface portal, to avoid redundant requests.
    last_surface_states: HashMap<String, bool>,
    style_animations: HashMap<String, StyleAnimation>,
}

#[derive(Debug)]
struct EmbeddedFrontendRuntime {
    module_id: String,
    script_ctx: ScriptContext,
}

impl FrontendSurfaceComponent {
    pub(super) fn new(
        compiled: CompiledFrontendModule,
        module_dir: PathBuf,
        frontend_catalog: FrontendCatalog,
        interface_catalog: mesh_core_service::InterfaceCatalog,
    ) -> Self {
        let module_settings_file = module_dir.join("config/settings.json");
        let settings_state =
            load_frontend_module_settings(&module_settings_file, &compiled.manifest);
        Self {
            compiled,
            module_dir,
            module_settings_file,
            settings_json: settings_state.raw,
            surface_layout: settings_state.layout.clone(),
            frontend_catalog,
            visible: settings_state.layout.visible_on_start,
            dirty: true,
            last_service_update: None,
            focused_key: None,
            pointer_down_key: None,
            active_slider_key: None,
            input_values: HashMap::new(),
            slider_values: HashMap::new(),
            checked_values: HashMap::new(),
            render_hooks_pending: true,
            scroll_offsets: HashMap::new(),
            hovered_key: None,
            hovered_path: Vec::new(),
            hovered_pos: (0.0, 0.0),
            hover_start: None,
            runtimes: Arc::new(Mutex::new(HashMap::new())),
            render_stack: RefCell::new(Vec::new()),
            active_theme: RefCell::new(default_theme()),
            measured_size: None,
            last_surface_size: None,
            locale: LocaleEngine::new("en"),
            interface_catalog,
            last_tree: None,
            diagnostics: None,
            pending_surface_states: RefCell::new(HashMap::new()),
            last_surface_states: HashMap::new(),
            style_animations: HashMap::new(),
        }
    }
}

fn tracked_service_fields_changed(
    previous: Option<&serde_json::Value>,
    next: &serde_json::Value,
    tracked_fields: &HashSet<String>,
) -> bool {
    tracked_fields.iter().any(|field| {
        let previous_value = previous.and_then(|value| value.get(field));
        let next_value = next.get(field);
        previous_value != next_value
    })
}

pub(super) fn grant_capabilities_from_manifest(
    manifest: &mesh_core_module::Manifest,
) -> CapabilitySet {
    let mut granted = CapabilitySet::new();

    for capability in &manifest.capabilities.required {
        granted.grant(Capability::new(capability.clone()));
    }

    for capability in &manifest.capabilities.optional {
        granted.grant(Capability::new(capability.clone()));
    }

    granted
}

#[cfg(test)]
mod tests;
