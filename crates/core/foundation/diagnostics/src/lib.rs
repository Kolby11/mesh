/// Logging, health reporting, and performance monitoring for MESH modules.
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

/// Health status of a module.
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

/// Per-module performance metrics.
#[derive(Debug, Clone)]
pub struct ModuleMetrics {
    pub module_id: String,
    pub avg_frame_time: Duration,
    pub peak_frame_time: Duration,
    pub memory_bytes: u64,
    pub error_count: u64,
    pub health: HealthStatus,
}

/// Diagnostics handle given to each module.
#[derive(Debug, Clone)]
pub struct Diagnostics {
    module_id: String,
    state: Arc<Mutex<DiagnosticsState>>,
}

/// A deduplicated record for a repeated backend lifecycle failure.
/// Keyed by `(provider_id, stage)`; repeats update `count` and `last_seen`.
#[derive(Debug, Clone)]
pub struct LifecycleErrorRecord {
    pub provider_id: String,
    pub stage: String,
    pub latest_message: String,
    pub count: u64,
    pub last_seen: SystemTime,
}

#[derive(Debug)]
struct DiagnosticsState {
    health: HealthStatus,
    error_count: u64,
    handler_errors: HashSet<(String, String, String)>,
    lifecycle_errors: HashMap<(String, String), LifecycleErrorRecord>,
    missing_icons: HashSet<(String, String)>,
}

impl Diagnostics {
    pub fn new(module_id: impl Into<String>) -> Self {
        Self {
            module_id: module_id.into(),
            state: Arc::new(Mutex::new(DiagnosticsState {
                health: HealthStatus::Healthy,
                error_count: 0,
                handler_errors: HashSet::new(),
                lifecycle_errors: HashMap::new(),
                missing_icons: HashSet::new(),
            })),
        }
    }

    pub fn module_id(&self) -> &str {
        &self.module_id
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

    pub fn record_missing_icon(
        &self,
        semantic_name: impl Into<String>,
        tried: Vec<String>,
    ) -> bool {
        let semantic_name = semantic_name.into();
        let mut state = self.state.lock().unwrap();
        let inserted = state
            .missing_icons
            .insert((self.module_id.clone(), semantic_name.clone()));
        if inserted {
            let tried = if tried.is_empty() {
                "no configured candidates".to_string()
            } else {
                format!("tried {}", tried.join(", "))
            };
            state.health = HealthStatus::Degraded(format!(
                "missing icon '{semantic_name}' for module '{}': {tried}",
                self.module_id
            ));
        }
        inserted
    }

    pub fn record_lifecycle_error(
        &self,
        provider_id: impl Into<String>,
        stage: impl Into<String>,
        message: impl Into<String>,
    ) -> bool {
        let provider_id = provider_id.into();
        let stage = stage.into();
        let message = message.into();
        let mut state = self.state.lock().unwrap();
        let key = (provider_id.clone(), stage.clone());
        if let Some(record) = state.lifecycle_errors.get_mut(&key) {
            // Repeat: update metadata only, do not increment unique error count.
            record.latest_message = message;
            record.count += 1;
            record.last_seen = SystemTime::now();
            false
        } else {
            // First occurrence: create bucket, set health, increment error count.
            state.lifecycle_errors.insert(
                key,
                LifecycleErrorRecord {
                    provider_id: provider_id.clone(),
                    stage: stage.clone(),
                    latest_message: message.clone(),
                    count: 1,
                    last_seen: SystemTime::now(),
                },
            );
            state.health = HealthStatus::Error(format!(
                "backend lifecycle '{stage}' failed for provider '{provider_id}': {message}"
            ));
            state.error_count += 1;
            true
        }
    }

    /// Return a snapshot of all lifecycle error records for this diagnostics handle.
    pub fn lifecycle_error_records(&self) -> Vec<LifecycleErrorRecord> {
        self.state
            .lock()
            .unwrap()
            .lifecycle_errors
            .values()
            .cloned()
            .collect()
    }

    pub fn health(&self) -> HealthStatus {
        self.state.lock().unwrap().health.clone()
    }

    pub fn error_count(&self) -> u64 {
        self.state.lock().unwrap().error_count
    }
}

/// Central diagnostics collector that aggregates metrics from all modules.
#[derive(Debug, Default)]
pub struct DiagnosticsCollector {
    modules: Vec<Diagnostics>,
}

impl DiagnosticsCollector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a module and return its diagnostics handle.
    pub fn register(&mut self, module_id: impl Into<String>) -> Diagnostics {
        let diag = Diagnostics::new(module_id);
        self.modules.push(diag.clone());
        diag
    }

    pub fn record_lifecycle_error(
        &mut self,
        provider_id: impl Into<String>,
        stage: impl Into<String>,
        message: impl Into<String>,
    ) -> bool {
        let provider_id = provider_id.into();
        let diagnostics = self
            .modules
            .iter()
            .find(|diagnostics| diagnostics.module_id() == provider_id)
            .cloned()
            .unwrap_or_else(|| self.register(provider_id.clone()));
        diagnostics.record_lifecycle_error(provider_id, stage, message)
    }

    /// Snapshot the health of all registered modules.
    pub fn snapshot(&self) -> Vec<(String, HealthStatus)> {
        self.modules
            .iter()
            .map(|d| (d.module_id().to_string(), d.health()))
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

    #[test]
    fn missing_icon_diagnostics_are_deduplicated_by_module_and_semantic_name() {
        let diagnostics = Diagnostics::new("@mesh/quick-settings");

        assert!(
            diagnostics.record_missing_icon("audio-volume-muted", vec!["material:nope".into()])
        );
        assert!(
            !diagnostics.record_missing_icon("audio-volume-muted", vec!["material:nope".into()])
        );
        assert!(diagnostics.record_missing_icon("network-wireless", vec!["material:nope".into()]));

        assert_eq!(diagnostics.error_count(), 0);
        match diagnostics.health() {
            HealthStatus::Degraded(message) => {
                assert!(message.contains("network-wireless"));
                assert!(message.contains("material:nope"));
            }
            other => panic!("expected degraded health, got {other:?}"),
        }
    }

    #[test]
    fn lifecycle_errors_are_deduplicated_by_provider_stage_and_message() {
        let diagnostics = Diagnostics::new("@mesh/pipewire-audio");

        assert!(diagnostics.record_lifecycle_error("@mesh/pipewire-audio", "poll_failed", "boom"));
        assert!(!diagnostics.record_lifecycle_error("@mesh/pipewire-audio", "poll_failed", "boom"));
        assert!(diagnostics.record_lifecycle_error("@mesh/pipewire-audio", "init_failed", "boom"));

        assert_eq!(diagnostics.error_count(), 2);
        match diagnostics.health() {
            HealthStatus::Error(message) => {
                assert!(message.contains("@mesh/pipewire-audio"));
                assert!(message.contains("init_failed"));
            }
            other => panic!("expected error health, got {other:?}"),
        }
    }

    #[test]
    fn lifecycle_errors_are_deduplicated_by_provider_and_stage() {
        let diagnostics = Diagnostics::new("@mesh/pipewire-audio");

        // First occurrence returns true and sets health.
        assert!(diagnostics.record_lifecycle_error("@mesh/pipewire-audio", "poll", "msg A"));
        assert_eq!(diagnostics.error_count(), 1);

        // Different stage is a new bucket.
        assert!(diagnostics.record_lifecycle_error("@mesh/pipewire-audio", "init", "msg A"));
        assert_eq!(diagnostics.error_count(), 2);

        // Same (provider, stage) with different message is NOT a new bucket.
        assert!(!diagnostics.record_lifecycle_error("@mesh/pipewire-audio", "poll", "msg B"));
        assert_eq!(diagnostics.error_count(), 2);

        // Different provider, same stage is a new bucket.
        assert!(diagnostics.record_lifecycle_error("@mesh/pulseaudio", "poll", "msg A"));
        assert_eq!(diagnostics.error_count(), 3);
    }

    #[test]
    fn repeated_lifecycle_failures_increment_count_without_new_error() {
        let diagnostics = Diagnostics::new("@mesh/pipewire-audio");

        // First occurrence.
        assert!(diagnostics.record_lifecycle_error("@mesh/pipewire-audio", "poll", "boom 1"));
        assert_eq!(diagnostics.error_count(), 1);

        // Second occurrence — different message, same (provider, stage) bucket.
        assert!(!diagnostics.record_lifecycle_error("@mesh/pipewire-audio", "poll", "boom 2"));
        assert_eq!(diagnostics.error_count(), 1);

        // Third occurrence.
        assert!(!diagnostics.record_lifecycle_error("@mesh/pipewire-audio", "poll", "boom 3"));
        assert_eq!(diagnostics.error_count(), 1);

        // Record count should be 3.
        let records = diagnostics.lifecycle_error_records();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].count, 3);
    }

    #[test]
    fn lifecycle_error_record_keeps_latest_message() {
        let diagnostics = Diagnostics::new("@mesh/pipewire-audio");

        assert!(diagnostics.record_lifecycle_error(
            "@mesh/pipewire-audio",
            "poll",
            "first message"
        ));
        assert!(!diagnostics.record_lifecycle_error(
            "@mesh/pipewire-audio",
            "poll",
            "second message"
        ));
        assert!(!diagnostics.record_lifecycle_error(
            "@mesh/pipewire-audio",
            "poll",
            "third message"
        ));

        let records = diagnostics.lifecycle_error_records();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].latest_message, "third message");
        assert_eq!(records[0].provider_id, "@mesh/pipewire-audio");
        assert_eq!(records[0].stage, "poll");
        assert_eq!(records[0].count, 3);
    }
}
