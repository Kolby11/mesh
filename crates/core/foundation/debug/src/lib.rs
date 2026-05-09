/// Debug overlay types for the MESH shell.
///
/// `DebugSnapshot` is a point-in-time view of shell internals built by the
/// core and handed to the renderer to paint over live surfaces.

/// A point-in-time snapshot of shell state for the debug overlay.
#[derive(Debug, Clone, Default)]
pub struct DebugSnapshot {
    pub modules: Vec<ModuleEntry>,
    pub interfaces: Vec<InterfaceEntry>,
    pub backend_runtimes: Vec<BackendRuntimeEntry>,
    pub health: Vec<HealthEntry>,
    pub active_surfaces: Vec<String>,
    pub benchmarks: DebugBenchmarkSnapshot,
    pub profiling: Option<ProfilingSnapshot>,
}

pub const DEBUG_INTERFACE: &str = "mesh.debug";
pub const DEBUG_SOURCE_MODULE_ID: &str = "@mesh/core-debug";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DebugInspectorView {
    #[default]
    Overview,
    Surfaces,
    BackendServices,
    Benchmark,
}

impl DebugInspectorView {
    pub fn label(self) -> &'static str {
        match self {
            Self::Overview => "overview",
            Self::Surfaces => "surfaces",
            Self::BackendServices => "backend_services",
            Self::Benchmark => "benchmark",
        }
    }

    pub fn from_legacy_tab(tab: DebugTab) -> Self {
        match tab {
            DebugTab::Modules => Self::Overview,
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
    Hover,
    SurfaceOpenClose,
    PointerUpdate,
    KeyboardTraversal,
    BackendUpdate,
}

impl BenchmarkScenarioId {
    pub fn id(self) -> &'static str {
        match self {
            Self::Hover => "hover",
            Self::SurfaceOpenClose => "surface_open_close",
            Self::PointerUpdate => "pointer_update",
            Self::KeyboardTraversal => "keyboard_traversal",
            Self::BackendUpdate => "backend_update",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Hover => "Hover",
            Self::SurfaceOpenClose => "Surface open/close",
            Self::PointerUpdate => "Pointer-driven update",
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
    pub duration_micros: u64,
    pub trigger_kind: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ProfilingSample {
    pub stage: ProfilingStage,
    pub order: u64,
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
    TreeBuild,
    StyleRestyle,
    Layout,
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
            Self::TreeBuild => "tree_build",
            Self::StyleRestyle => "style_restyle",
            Self::Layout => "layout",
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
    pub active_tab: DebugTab,
    pub active_view: DebugInspectorView,
    pub profiling_enabled: bool,
    pub profiling_session_id: u64,
    pub latest_benchmark_run: Option<DebugBenchmarkRunState>,
}

impl DebugOverlayState {
    pub fn toggle(&mut self) {
        self.enabled = !self.enabled;
    }

    pub fn cycle_tab(&mut self) {
        self.active_view = match self.active_view {
            DebugInspectorView::Overview => DebugInspectorView::Surfaces,
            DebugInspectorView::Surfaces => DebugInspectorView::BackendServices,
            DebugInspectorView::BackendServices => DebugInspectorView::Benchmark,
            DebugInspectorView::Benchmark => DebugInspectorView::Overview,
        };
        self.active_tab = match self.active_view {
            DebugInspectorView::Overview => DebugTab::Modules,
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
