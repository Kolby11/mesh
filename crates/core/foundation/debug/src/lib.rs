/// Debug overlay types for the MESH shell.
///
/// `DebugSnapshot` is a point-in-time view of shell internals built by the
/// core and handed to the renderer to paint over live surfaces.

/// A point-in-time snapshot of shell state for the debug overlay.
#[derive(Debug, Clone, Default)]
pub struct DebugSnapshot {
    pub modules: Vec<ModuleEntry>,
    pub module_graph: Vec<ModuleGraphEntry>,
    pub module_instances: Vec<ModuleObjectEntry>,
    pub interfaces: Vec<InterfaceEntry>,
    pub backend_runtimes: Vec<BackendRuntimeEntry>,
    pub method_calls: Vec<MethodCallEntry>,
    pub health: Vec<HealthEntry>,
    pub keybinds: Vec<DebugKeybindEntry>,
    pub active_surfaces: Vec<String>,
    pub benchmarks: DebugBenchmarkSnapshot,
    pub profiling: Option<ProfilingSnapshot>,
}

pub const DEBUG_INTERFACE: &str = "mesh.debug";
pub const DEBUG_SOURCE_MODULE_ID: &str = "@mesh/core-debug";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DebugKeybindEntry {
    pub surface_id: String,
    pub module_id: String,
    pub action_id: String,
    pub label: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub label_key: Option<String>,
    pub description_key: Option<String>,
    pub category_key: Option<String>,
    pub key: String,
    pub modifiers: Vec<String>,
    pub trigger_kind: String,
    pub source: String,
    pub accessibility_shortcut: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DebugInspectorView {
    #[default]
    Overview,
    Modules,
    Surfaces,
    BackendServices,
    Benchmark,
}

impl DebugInspectorView {
    pub fn label(self) -> &'static str {
        match self {
            Self::Overview => "overview",
            Self::Modules => "modules",
            Self::Surfaces => "surfaces",
            Self::BackendServices => "backend_services",
            Self::Benchmark => "benchmark",
        }
    }

    pub fn from_legacy_tab(tab: DebugTab) -> Self {
        match tab {
            DebugTab::Modules => Self::Modules,
            DebugTab::Interfaces => Self::Surfaces,
            DebugTab::Health => Self::BackendServices,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DebugBenchmarkSnapshot {
    pub scenarios: Vec<BenchmarkScenarioSnapshot>,
}

#[derive(Debug, Clone)]
pub struct BenchmarkScenarioSnapshot {
    pub id: BenchmarkScenarioId,
    pub label: String,
    pub target: String,
    pub status: BenchmarkScenarioStatus,
    pub primary_metric: String,
    pub secondary_metric: String,
    pub hint: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BenchmarkScenarioId {
    Idle,
    Hover,
    SurfaceOpenClose,
    PointerUpdate,
    TextUpdate,
    Scroll,
    IconGrid,
    Animation,
    ThemeReload,
    Resize,
    KeyboardTraversal,
    BackendUpdate,
}

impl BenchmarkScenarioId {
    pub fn id(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Hover => "hover",
            Self::SurfaceOpenClose => "surface_open_close",
            Self::PointerUpdate => "pointer_update",
            Self::TextUpdate => "text_update",
            Self::Scroll => "scroll",
            Self::IconGrid => "icon_grid",
            Self::Animation => "animation",
            Self::ThemeReload => "theme_reload",
            Self::Resize => "resize",
            Self::KeyboardTraversal => "keyboard_traversal",
            Self::BackendUpdate => "backend_update",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "Idle scheduler",
            Self::Hover => "Hover",
            Self::SurfaceOpenClose => "Surface open/close",
            Self::PointerUpdate => "Pointer move",
            Self::TextUpdate => "Text update",
            Self::Scroll => "Scroll",
            Self::IconGrid => "Icon grid",
            Self::Animation => "Animation tick",
            Self::ThemeReload => "Theme reload",
            Self::Resize => "Resize",
            Self::KeyboardTraversal => "Keyboard traversal",
            Self::BackendUpdate => "Backend-driven update",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BenchmarkScenarioStatus {
    ProfilingOff,
    Ready,
    Running,
    WaitingForSamples,
    Complete,
    Unavailable,
    Skipped,
}

impl BenchmarkScenarioStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::ProfilingOff => "Profiling off",
            Self::Ready => "Ready",
            Self::Running => "Running",
            Self::WaitingForSamples => "Waiting for samples",
            Self::Complete => "Complete",
            Self::Unavailable => "Unavailable",
            Self::Skipped => "Skipped",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebugBenchmarkRunState {
    pub scenario_id: BenchmarkScenarioId,
    pub status: BenchmarkScenarioStatus,
}

#[derive(Debug, Clone, Default)]
pub struct ProfilingSnapshot {
    pub session_id: u64,
    pub shell: ProfilingScopeSnapshot,
    pub surfaces: Vec<ProfilingSurfaceSnapshot>,
    pub backends: Vec<ProfilingBackendSnapshot>,
}

#[derive(Debug, Clone, Default)]
pub struct ProfilingScopeSnapshot {
    pub stages: Vec<ProfilingStageSummary>,
    pub redraw_count: u64,
    pub total_surface_render_time_micros: u64,
}

#[derive(Debug, Clone, Default)]
pub struct ProfilingSurfaceSnapshot {
    pub surface_id: String,
    pub module_id: Option<String>,
    pub stages: Vec<ProfilingStageSummary>,
    pub redraw_count: u64,
    pub total_surface_render_time_micros: u64,
    pub invalidation: Option<ProfilingInvalidationSnapshot>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfilingInvalidationSnapshot {
    pub full_rebuild: bool,
    pub retained_path: bool,
    pub retained_generation: u64,
    pub component: ComponentInvalidationCounts,
    pub retained: RetainedInvalidationCounts,
    pub paint: RetainedPaintSnapshot,
    pub text: TextCacheSnapshot,
    /// True when the paint pass took the narrow-script path (SCRIPT_NARROW flag).
    pub narrow_path: bool,
    /// Number of nodes (expanded leaf + ancestor chain) dirtied on the narrow path.
    pub affected_node_count: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ComponentInvalidationCounts {
    pub script: u64,
    pub state: u64,
    pub style: u64,
    pub layout: u64,
    pub paint: u64,
    pub text: u64,
    pub accessibility: u64,
    pub metrics: u64,
    pub surface_config: u64,
    /// Incremented when a SCRIPT_NARROW invalidation was taken (leaf-only path).
    pub script_narrow: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RetainedInvalidationCounts {
    pub inserted: u64,
    pub removed: u64,
    pub layout: u64,
    pub style: u64,
    pub attributes: u64,
    pub children: u64,
    pub state: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RetainedPaintSnapshot {
    pub retained_generation: u64,
    pub entries_total: u64,
    pub entries_reused: u64,
    pub entries_rebuilt: u64,
    pub entries_removed: u64,
    pub subtree_segments_reused: u64,
    pub subtree_segments_rebuilt: u64,
    pub subtree_commands_rebuilt: u64,
    pub changed_layout_count: u64,
    pub changed_paint_count: u64,
    pub effect_overflow_count: u64,
    pub fallback_promotion_count: u64,
    pub full_fallback_count: u64,
    pub broad_dirty_fallback_count: u64,
    pub damage_rect_count: u64,
    pub damage_area: u64,
    pub surface_area: u64,
    pub full_surface_damage: bool,
    pub partial_present_supported: bool,
    pub skipped_paint_pixels: u64,
    pub omitted_subtrees: u64,
    pub omitted_nodes: u64,
    pub omitted_commands: u64,
    pub preclipped_descendants: u64,
    pub repaint_policy: RepaintPolicySnapshot,
    pub filtered_span_count: u64,
    pub filtered_command_count: u64,
    pub filtered_commands_skipped: u64,
    pub filtered_fallback_count: u64,
    pub batch_count: u64,
    pub batched_primitives: u64,
    pub barrier_count: u64,
    pub barriers: DisplayBatchBarrierSnapshot,
    pub raster_cache_hits: u64,
    pub raster_cache_misses: u64,
    pub raster_cache_bypasses: u64,
    pub raster_cache_opaque_hits: u64,
    pub raster_cache_translucent_hits: u64,
    pub glyph_cache_hits: u64,
    pub glyph_cache_misses: u64,
    pub glyph_cache_entries: u64,
    pub glyph_cache_capacity: u64,
    pub font_bytes_cache_hits: u64,
    pub font_bytes_cache_misses: u64,
    pub font_bytes_cache_entries: u64,
    pub font_bytes_cache_capacity: u64,
    pub skia_glyph_cache_hits: u64,
    pub skia_glyph_cache_misses: u64,
    pub skia_glyph_cache_entries: u64,
    pub skia_glyph_cache_capacity: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RepaintPolicySnapshot {
    MinimalDamage,
    BoundingRect,
    #[default]
    FullSurface,
}

impl RepaintPolicySnapshot {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MinimalDamage => "minimal_damage",
            Self::BoundingRect => "bounding_rect",
            Self::FullSurface => "full_surface",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DisplayBatchBarrierSnapshot {
    pub text: u64,
    pub icon: u64,
    pub opacity: u64,
    pub clip: u64,
    pub translucency: u64,
    pub material_change: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TextCacheSnapshot {
    pub layout_hits: u64,
    pub layout_misses: u64,
    pub layout_invalidations: u64,
    pub shaped_entries: u64,
    pub glyph_cache_active: bool,
    pub shaping_micros: u64,
}

#[derive(Debug, Clone, Default)]
pub struct ProfilingStageSummary {
    pub stage: ProfilingStage,
    pub sample_count: u64,
    pub total_micros: u64,
    pub max_micros: u64,
    pub recent_samples: Vec<ProfilingSample>,
}

#[derive(Debug, Clone, Default)]
pub struct ProfilingBackendSnapshot {
    pub interface: String,
    pub provider_id: String,
    pub stages: Vec<ProfilingBackendStageSummary>,
}

#[derive(Debug, Clone, Default)]
pub struct ProfilingBackendStageSummary {
    pub stage: ProfilingBackendStage,
    pub sample_count: u64,
    pub total_micros: u64,
    pub max_micros: u64,
    pub recent_samples: Vec<ProfilingBackendSample>,
}

#[derive(Debug, Clone, Default)]
pub struct ProfilingBackendSample {
    pub stage: ProfilingBackendStage,
    pub order: u64,
    /// Monotonic microseconds since profiling was enabled.
    pub timestamp_micros: u64,
    pub duration_micros: u64,
    pub trigger_kind: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ProfilingSample {
    pub stage: ProfilingStage,
    pub order: u64,
    /// Monotonic microseconds since profiling was enabled.
    pub timestamp_micros: u64,
    pub duration_micros: u64,
    pub surface_id: Option<String>,
    pub module_id: Option<String>,
    pub redraw_count: Option<u32>,
    pub trigger_kind: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ProfilingBackendStage {
    PollUpdate,
    CommandHandling,
    #[default]
    StatePublishDelivery,
}

impl ProfilingBackendStage {
    pub fn label(self) -> &'static str {
        match self {
            Self::PollUpdate => "poll_update",
            Self::CommandHandling => "command_handling",
            Self::StatePublishDelivery => "state_publish_delivery",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ProfilingStage {
    InputHandling,
    RuntimeUpdateHandling,
    SchedulerIdle,
    TreeBuild,
    StyleRestyle,
    Layout,
    RenderObjectSync,
    RetainedDisplayListUpdate,
    PaintTraversal,
    TextShaping,
    IconImageRaster,
    Paint,
    PresentCommit,
    RedrawCount,
    #[default]
    TotalSurfaceRender,
}

impl ProfilingStage {
    pub fn label(self) -> &'static str {
        match self {
            Self::InputHandling => "input_handling",
            Self::RuntimeUpdateHandling => "runtime_update_handling",
            Self::SchedulerIdle => "scheduler_idle",
            Self::TreeBuild => "tree_build",
            Self::StyleRestyle => "style_restyle",
            Self::Layout => "layout",
            Self::RenderObjectSync => "render_object_sync",
            Self::RetainedDisplayListUpdate => "retained_display_list_update",
            Self::PaintTraversal => "paint_traversal",
            Self::TextShaping => "text_shaping",
            Self::IconImageRaster => "icon_image_raster",
            Self::Paint => "paint",
            Self::PresentCommit => "present_commit",
            Self::RedrawCount => "redraw_count",
            Self::TotalSurfaceRender => "total_surface_render",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ModuleEntry {
    pub id: String,
    pub module_type: String,
    pub state: String,
    pub error_count: u32,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModuleGraphEntry {
    pub module_id: String,
    pub kind: String,
    pub enabled: bool,
    pub path: String,
    pub uses_modules: Vec<String>,
    pub uses_interfaces: Vec<String>,
    pub uses_optional_interfaces: Vec<String>,
    pub uses_icon_packs: Vec<String>,
    pub uses_i18n_packs: Vec<String>,
    pub uses_theme_packs: Vec<String>,
    pub uses_font_packs: Vec<String>,
    pub required_binaries: Vec<String>,
    pub optional_binaries: Vec<String>,
    pub keybind_actions: Vec<String>,
    /// Resolved `interface=provider` pairs for interfaces consumed by this module.
    pub active_providers: Vec<String>,
    pub native_binaries: Vec<ModuleBinaryHealthEntry>,
    pub capabilities: Vec<String>,
    pub optional_capabilities: Vec<String>,
    pub surface_entrypoint: Option<String>,
    pub surface_settings_namespace: Option<String>,
    pub surface_accessibility_role: Option<String>,
    pub surface_accessibility_label: Option<String>,
    pub surface_size_policy: Option<String>,
    pub surface_layout_label: Option<String>,
    pub surface_layout_label_key: Option<String>,
    pub surface_layout_label_fallback: Option<String>,
    pub provides_interfaces: Vec<String>,
    /// Resolved display labels for each provided interface, indexed parallel to `provides_interfaces`.
    pub provides_interface_labels: Vec<Option<String>>,
    pub provides_settings: Vec<String>,
    pub provides_i18n: Vec<String>,
    pub provides_themes: Vec<String>,
    pub provides_theme_labels: Vec<Option<String>>,
    pub required_icons: Vec<String>,
    pub optional_icons: Vec<String>,
    pub diagnostics: Vec<String>,
    pub health: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleBinaryHealthEntry {
    pub name: String,
    pub optional: bool,
    pub available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleObjectEntry {
    pub instance_id: String,
    pub module_id: String,
    pub object_kind: String,
    pub interface: Option<String>,
    pub version: Option<String>,
    pub lifecycle: String,
    pub active: bool,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct InterfaceEntry {
    pub name: String,
    pub providers: Vec<ProviderEntry>,
}

#[derive(Debug, Clone)]
pub struct ProviderEntry {
    pub backend_name: String,
    pub priority: u32,
}

#[derive(Debug, Clone)]
pub struct BackendRuntimeEntry {
    pub interface: String,
    pub provider_id: String,
    pub status: String,
    pub message: String,
    /// Number of times this (interface, provider_id) pair has recorded a failure.
    /// Zero for non-failure entries.
    pub failure_count: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MethodCallEntry {
    pub interface: String,
    pub provider_id: Option<String>,
    pub source_module_id: String,
    pub command: String,
    pub status: String,
    pub queued: bool,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HealthEntry {
    pub module_id: String,
    pub status: String,
}

/// Runtime state of the debug overlay — owned by the shell.
#[derive(Debug, Default)]
pub struct DebugOverlayState {
    pub enabled: bool,
    pub show_layout_bounds: bool,
    /// Chrome-style element picker. While active, pointer input is captured
    /// by the shell and the deepest node under the cursor is highlighted.
    pub element_picker_enabled: bool,
    /// Last node under the picker cursor, also retained after click selection.
    pub inspected_element: Option<serde_json::Value>,
    pub active_tab: DebugTab,
    pub active_view: DebugInspectorView,
    pub profiling_enabled: bool,
    pub profiling_session_id: u64,
    pub latest_benchmark_run: Option<DebugBenchmarkRunState>,
    pub recent_method_calls: Vec<MethodCallEntry>,
}

impl DebugOverlayState {
    pub fn toggle(&mut self) {
        self.enabled = !self.enabled;
    }

    pub fn toggle_layout_bounds(&mut self) {
        self.show_layout_bounds = !self.show_layout_bounds;
    }

    pub fn toggle_element_picker(&mut self) {
        self.element_picker_enabled = !self.element_picker_enabled;
        if self.element_picker_enabled {
            self.inspected_element = None;
        }
    }

    pub fn cycle_tab(&mut self) {
        self.active_view = match self.active_view {
            DebugInspectorView::Overview => DebugInspectorView::Modules,
            DebugInspectorView::Modules => DebugInspectorView::Surfaces,
            DebugInspectorView::Surfaces => DebugInspectorView::BackendServices,
            DebugInspectorView::BackendServices => DebugInspectorView::Benchmark,
            DebugInspectorView::Benchmark => DebugInspectorView::Overview,
        };
        self.active_tab = match self.active_view {
            DebugInspectorView::Overview | DebugInspectorView::Modules => DebugTab::Modules,
            DebugInspectorView::Surfaces => DebugTab::Interfaces,
            DebugInspectorView::BackendServices | DebugInspectorView::Benchmark => DebugTab::Health,
        };
    }

    pub fn toggle_profiling(&mut self) -> bool {
        self.profiling_enabled = !self.profiling_enabled;
        if self.profiling_enabled {
            self.profiling_session_id = self.profiling_session_id.saturating_add(1);
        }
        self.profiling_enabled
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum DebugTab {
    #[default]
    Modules,
    Interfaces,
    Health,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_tab_reaches_benchmark_view() {
        let mut state = DebugOverlayState::default();

        state.cycle_tab();
        assert_eq!(state.active_view, DebugInspectorView::Modules);
        state.cycle_tab();
        assert_eq!(state.active_view, DebugInspectorView::Surfaces);
        state.cycle_tab();
        assert_eq!(state.active_view, DebugInspectorView::BackendServices);
        state.cycle_tab();
        assert_eq!(state.active_view, DebugInspectorView::Benchmark);
        state.cycle_tab();
        assert_eq!(state.active_view, DebugInspectorView::Overview);
    }
}

impl DebugTab {
    pub fn label(self) -> &'static str {
        match self {
            Self::Modules => "Modules",
            Self::Interfaces => "Interfaces",
            Self::Health => "Health",
        }
    }
}
