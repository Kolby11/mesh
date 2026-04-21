/// Debug overlay types for the MESH shell.
///
/// `DebugSnapshot` is a point-in-time view of shell internals built by the
/// core and handed to the renderer to paint over live surfaces.

/// A point-in-time snapshot of shell state for the debug overlay.
#[derive(Debug, Clone, Default)]
pub struct DebugSnapshot {
    pub plugins: Vec<PluginEntry>,
    pub interfaces: Vec<InterfaceEntry>,
    pub health: Vec<HealthEntry>,
    pub active_surfaces: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct PluginEntry {
    pub id: String,
    pub plugin_type: String,
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
pub struct HealthEntry {
    pub plugin_id: String,
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
            DebugTab::Plugins => DebugTab::Interfaces,
            DebugTab::Interfaces => DebugTab::Health,
            DebugTab::Health => DebugTab::Plugins,
        };
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum DebugTab {
    #[default]
    Plugins,
    Interfaces,
    Health,
}

impl DebugTab {
    pub fn label(self) -> &'static str {
        match self {
            Self::Plugins => "Plugins",
            Self::Interfaces => "Interfaces",
            Self::Health => "Health",
        }
    }
}
