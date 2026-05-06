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
}

impl DebugOverlayState {
    pub fn toggle(&mut self) {
        self.enabled = !self.enabled;
    }

    pub fn cycle_tab(&mut self) {
        self.active_tab = match self.active_tab {
            DebugTab::Modules => DebugTab::Interfaces,
            DebugTab::Interfaces => DebugTab::Health,
            DebugTab::Health => DebugTab::Modules,
        };
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum DebugTab {
    #[default]
    Modules,
    Interfaces,
    Health,
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
