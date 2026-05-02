/// Logging, health reporting, and performance monitoring for MESH plugins.
use std::collections::HashSet;
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
    handler_errors: HashSet<(String, String, String)>,
}

impl Diagnostics {
    pub fn new(plugin_id: impl Into<String>) -> Self {
        Self {
            plugin_id: plugin_id.into(),
            state: Arc::new(Mutex::new(DiagnosticsState {
                health: HealthStatus::Healthy,
                error_count: 0,
                handler_errors: HashSet::new(),
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

    pub fn record_handler_error(
        &self,
        component_id: impl Into<String>,
        handler_name: impl Into<String>,
        message: impl Into<String>,
    ) -> bool {
        let component_id = component_id.into();
        let handler_name = handler_name.into();
        let message = message.into();
        let mut state = self.state.lock().unwrap();
        let inserted = state.handler_errors.insert((
            component_id.clone(),
            handler_name.clone(),
            message.clone(),
        ));
        if inserted {
            state.health = HealthStatus::Error(format!(
                "handler '{handler_name}' failed in component '{component_id}': {message}"
            ));
            state.error_count += 1;
        }
        inserted
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handler_errors_are_deduplicated_by_component_handler_and_message() {
        let diagnostics = Diagnostics::new("@test/frontend");

        assert!(diagnostics.record_handler_error("@test/frontend", "onChange", "boom"));
        assert!(!diagnostics.record_handler_error("@test/frontend", "onChange", "boom"));
        assert!(diagnostics.record_handler_error("@test/frontend", "onRelease", "boom"));

        assert_eq!(diagnostics.error_count(), 2);
        assert!(matches!(diagnostics.health(), HealthStatus::Error(_)));
    }
}
