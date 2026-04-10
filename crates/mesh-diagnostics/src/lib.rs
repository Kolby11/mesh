/// Logging, health reporting, and performance monitoring for MESH plugins.
use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Health status of a plugin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthStatus {
    Healthy,
    Degraded(String),
    Error(String),
}

impl fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Healthy => write!(f, "healthy"),
            Self::Degraded(msg) => write!(f, "degraded: {msg}"),
            Self::Error(msg) => write!(f, "error: {msg}"),
        }
    }
}

/// Per-plugin performance metrics.
#[derive(Debug, Clone)]
pub struct PluginMetrics {
    pub plugin_id: String,
    pub avg_frame_time: Duration,
    pub peak_frame_time: Duration,
    pub memory_bytes: u64,
    pub error_count: u64,
    pub health: HealthStatus,
}

/// Diagnostics handle given to each plugin.
#[derive(Debug, Clone)]
pub struct Diagnostics {
    plugin_id: String,
    state: Arc<Mutex<DiagnosticsState>>,
}

#[derive(Debug)]
struct DiagnosticsState {
    health: HealthStatus,
    error_count: u64,
}

impl Diagnostics {
    pub fn new(plugin_id: impl Into<String>) -> Self {
        Self {
            plugin_id: plugin_id.into(),
            state: Arc::new(Mutex::new(DiagnosticsState {
                health: HealthStatus::Healthy,
                error_count: 0,
            })),
        }
    }

    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    pub fn healthy(&self) {
        let mut state = self.state.lock().unwrap();
        state.health = HealthStatus::Healthy;
    }

    pub fn degraded(&self, message: impl Into<String>) {
        let mut state = self.state.lock().unwrap();
        state.health = HealthStatus::Degraded(message.into());
    }

    pub fn error(&self, message: impl Into<String>) {
        let mut state = self.state.lock().unwrap();
        state.health = HealthStatus::Error(message.into());
        state.error_count += 1;
    }

    pub fn health(&self) -> HealthStatus {
        self.state.lock().unwrap().health.clone()
    }

    pub fn error_count(&self) -> u64 {
        self.state.lock().unwrap().error_count
    }
}

/// Central diagnostics collector that aggregates metrics from all plugins.
#[derive(Debug, Default)]
pub struct DiagnosticsCollector {
    plugins: Vec<Diagnostics>,
}

impl DiagnosticsCollector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a plugin and return its diagnostics handle.
    pub fn register(&mut self, plugin_id: impl Into<String>) -> Diagnostics {
        let diag = Diagnostics::new(plugin_id);
        self.plugins.push(diag.clone());
        diag
    }

    /// Snapshot the health of all registered plugins.
    pub fn snapshot(&self) -> Vec<(String, HealthStatus)> {
        self.plugins
            .iter()
            .map(|d| (d.plugin_id().to_string(), d.health()))
            .collect()
    }
}
