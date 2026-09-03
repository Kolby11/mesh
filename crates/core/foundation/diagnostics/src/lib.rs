//! Logging, health reporting, and performance monitoring for modules.

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime};

use serde::Serialize;

#[cfg(test)]
mod test_alloc {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::sync::atomic::{AtomicU64, Ordering};

    static ALLOCATION_COUNT: AtomicU64 = AtomicU64::new(0);
    static ALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);

    pub(super) struct CountingAllocator;

    pub(super) fn reset() {
        ALLOCATION_COUNT.store(0, Ordering::Relaxed);
        ALLOCATED_BYTES.store(0, Ordering::Relaxed);
    }

    pub(super) fn snapshot() -> (u64, u64) {
        (
            ALLOCATION_COUNT.load(Ordering::Relaxed),
            ALLOCATED_BYTES.load(Ordering::Relaxed),
        )
    }

    fn record(bytes: usize) {
        ALLOCATION_COUNT.fetch_add(1, Ordering::Relaxed);
        ALLOCATED_BYTES.fetch_add(bytes as u64, Ordering::Relaxed);
    }

    unsafe impl GlobalAlloc for CountingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let pointer = unsafe { System.alloc(layout) };
            if !pointer.is_null() {
                record(layout.size());
            }
            pointer
        }

        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            let pointer = unsafe { System.alloc_zeroed(layout) };
            if !pointer.is_null() {
                record(layout.size());
            }
            pointer
        }

        unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
            unsafe { System.dealloc(pointer, layout) };
        }

        unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            let new_pointer = unsafe { System.realloc(pointer, layout, new_size) };
            if !new_pointer.is_null() {
                record(new_size);
            }
            new_pointer
        }
    }
}

#[cfg(test)]
#[global_allocator]
static TEST_ALLOCATOR: test_alloc::CountingAllocator = test_alloc::CountingAllocator;

/// Stable categories shared by runtime and debug diagnostics.
///
/// Parser/compiler crates intentionally keep their own low-level dependency
/// boundary, so they convert their typed categories to these values at the
/// shell boundary. The serialized names are part of the debug/service API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticCategory {
    Runtime,
    Script,
    Interface,
    Storage,
    Parse,
    Template,
    Style,
    Props,
    Semantics,
    I18n,
    Import,
    Compilation,
    Validation,
    Source,
    Lifecycle,
    Configuration,
    Resource,
}

impl DiagnosticCategory {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Runtime => "runtime",
            Self::Script => "script",
            Self::Interface => "interface",
            Self::Storage => "storage",
            Self::Parse => "parse",
            Self::Template => "template",
            Self::Style => "style",
            Self::Props => "props",
            Self::Semantics => "semantics",
            Self::I18n => "i18n",
            Self::Import => "import",
            Self::Compilation => "compilation",
            Self::Validation => "validation",
            Self::Source => "source",
            Self::Lifecycle => "lifecycle",
            Self::Configuration => "configuration",
            Self::Resource => "resource",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct DiagnosticSourceSpan {
    pub start: usize,
    pub end: usize,
}

impl DiagnosticSourceSpan {
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

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

#[derive(Debug, Clone)]
pub struct ModuleMetrics {
    pub module_id: String,
    pub avg_frame_time: Duration,
    pub peak_frame_time: Duration,
    pub memory_bytes: u64,
    pub error_count: u64,
    pub health: HealthStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueSeverity {
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticIssue {
    pub module_id: String,
    pub instance_id: String,
    pub issue_code: String,
    pub category: DiagnosticCategory,
    pub severity: IssueSeverity,
    pub message: String,
    pub source_path: Option<String>,
    pub source_span: Option<DiagnosticSourceSpan>,
    pub first_seen: SystemTime,
    pub last_seen: SystemTime,
    pub count: u64,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticInstanceSnapshot {
    pub instance_id: String,
    pub health: HealthStatus,
    pub issues: Vec<DiagnosticIssue>,
    pub active_issues: Vec<DiagnosticIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticModuleSnapshot {
    pub module_id: String,
    pub health: HealthStatus,
    pub instances: Vec<DiagnosticInstanceSnapshot>,
}

#[derive(Debug, Clone)]
pub struct Diagnostics {
    module_id: String,
    instance_id: String,
    state: Arc<Mutex<DiagnosticsState>>,
    generation: Arc<AtomicU64>,
    #[cfg(test)]
    lock_acquisitions: Arc<AtomicU64>,
}

/// Compatibility view for callers that inspect backend lifecycle rows.
#[derive(Debug, Clone)]
pub struct LifecycleErrorRecord {
    pub provider_id: String,
    pub stage: String,
    pub latest_message: String,
    pub count: u64,
    pub last_seen: SystemTime,
    pub active: bool,
}

#[derive(Debug)]
struct DiagnosticsState {
    error_count: u64,
    issues: HashMap<String, DiagnosticIssue>,
}

impl Diagnostics {
    pub fn new(module_id: impl Into<String>) -> Self {
        Self::new_instance(module_id, "")
    }

    pub fn new_instance(module_id: impl Into<String>, instance_id: impl Into<String>) -> Self {
        Self::new_instance_with_generation(module_id, instance_id, Arc::new(AtomicU64::new(0)))
    }

    fn new_instance_with_generation(
        module_id: impl Into<String>,
        instance_id: impl Into<String>,
        generation: Arc<AtomicU64>,
    ) -> Self {
        Self {
            module_id: module_id.into(),
            instance_id: instance_id.into(),
            state: Arc::new(Mutex::new(DiagnosticsState {
                error_count: 0,
                issues: HashMap::new(),
            })),
            generation,
            #[cfg(test)]
            lock_acquisitions: Arc::new(AtomicU64::new(0)),
        }
    }

    fn lock_state(&self) -> MutexGuard<'_, DiagnosticsState> {
        #[cfg(test)]
        self.lock_acquisitions.fetch_add(1, Ordering::Relaxed);
        self.state.lock().unwrap()
    }

    fn cloned_issues(&self) -> Vec<DiagnosticIssue> {
        let state = self.lock_state();
        state.issues.values().cloned().collect()
    }

    #[cfg(test)]
    fn reset_lock_acquisitions(&self) {
        self.lock_acquisitions.store(0, Ordering::Relaxed);
    }

    #[cfg(test)]
    fn lock_acquisition_count(&self) -> u64 {
        self.lock_acquisitions.load(Ordering::Relaxed)
    }

    pub fn module_id(&self) -> &str {
        &self.module_id
    }

    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }

    /// Resolve only the generic runtime issue. Other active issues remain
    /// visible, so a successful operation cannot hide an unresolved fault.
    pub fn healthy(&self) {
        self.resolve_issue("runtime");
    }

    pub fn degraded(&self, message: impl Into<String>) {
        self.record_issue("runtime", IssueSeverity::Warning, message);
    }

    pub fn error(&self, message: impl Into<String>) {
        self.record_issue("runtime", IssueSeverity::Error, message);
    }

    /// Record or refresh one stable issue. The code, not the message, is the
    /// identity, so changing details updates one row instead of appending a
    /// duplicate. The return value is true only when the issue became active.
    pub fn record_issue(
        &self,
        issue_code: impl Into<String>,
        severity: IssueSeverity,
        message: impl Into<String>,
    ) -> bool {
        self.record_issue_inner(
            issue_code.into(),
            None,
            severity,
            message.into(),
            None,
            None,
        )
    }

    /// Record an issue with typed category and source metadata. Existing issue
    /// history remains deduplicated by `issue_code`, while the latest typed
    /// metadata is refreshed with the message.
    pub fn record_issue_with_source(
        &self,
        issue_code: impl Into<String>,
        category: DiagnosticCategory,
        severity: IssueSeverity,
        message: impl Into<String>,
        source_path: Option<String>,
        source_span: Option<DiagnosticSourceSpan>,
    ) -> bool {
        self.record_issue_inner(
            issue_code.into(),
            Some(category),
            severity,
            message.into(),
            source_path,
            source_span,
        )
    }

    fn record_issue_inner(
        &self,
        issue_code: String,
        category: Option<DiagnosticCategory>,
        severity: IssueSeverity,
        message: String,
        source_path: Option<String>,
        source_span: Option<DiagnosticSourceSpan>,
    ) -> bool {
        let now = SystemTime::now();
        let mut state = self.lock_state();
        let newly_active = if let Some(issue) = state.issues.get_mut(&issue_code) {
            let newly_active = !issue.active;
            let previous_severity = issue.severity;
            if newly_active {
                issue.severity = severity;
            } else {
                issue.severity = issue.severity.max(severity);
            }
            issue.message = message;
            if let Some(category) = category {
                issue.category = category;
            }
            if source_path.is_some() {
                issue.source_path = source_path;
            }
            if source_span.is_some() {
                issue.source_span = source_span;
            }
            issue.last_seen = now;
            issue.count = issue.count.saturating_add(1);
            issue.active = true;
            if issue.severity == IssueSeverity::Error
                && (newly_active || previous_severity != IssueSeverity::Error)
            {
                state.error_count = state.error_count.saturating_add(1);
            }
            newly_active
        } else {
            if severity == IssueSeverity::Error {
                state.error_count = state.error_count.saturating_add(1);
            }
            state.issues.insert(
                issue_code.clone(),
                DiagnosticIssue {
                    module_id: self.module_id.clone(),
                    instance_id: self.instance_id.clone(),
                    issue_code,
                    category: category.unwrap_or(DiagnosticCategory::Runtime),
                    severity,
                    message,
                    source_path,
                    source_span,
                    first_seen: now,
                    last_seen: now,
                    count: 1,
                    active: true,
                },
            );
            true
        };
        drop(state);
        self.generation.fetch_add(1, Ordering::Relaxed);
        newly_active
    }

    pub fn resolve_issue(&self, issue_code: &str) -> bool {
        let mut state = self.lock_state();
        let resolved = state.issues.get_mut(issue_code).is_some_and(|issue| {
            if !issue.active {
                return false;
            }
            issue.active = false;
            issue.last_seen = SystemTime::now();
            true
        });
        drop(state);
        if resolved {
            self.generation.fetch_add(1, Ordering::Relaxed);
        }
        resolved
    }

    pub fn record_handler_error(
        &self,
        component_id: impl Into<String>,
        handler_name: impl Into<String>,
        message: impl Into<String>,
    ) -> bool {
        self.record_handler_error_with_source(component_id, handler_name, message, None, None)
    }

    pub fn record_handler_error_with_source(
        &self,
        component_id: impl Into<String>,
        handler_name: impl Into<String>,
        message: impl Into<String>,
        source_path: Option<String>,
        source_span: Option<DiagnosticSourceSpan>,
    ) -> bool {
        let component_id = component_id.into();
        let handler_name = handler_name.into();
        self.record_issue_with_source(
            format!("handler:{component_id}:{handler_name}"),
            DiagnosticCategory::Script,
            IssueSeverity::Error,
            format!(
                "handler '{handler_name}' failed in component '{component_id}': {}",
                message.into()
            ),
            source_path,
            source_span,
        )
    }

    pub fn record_missing_icon(
        &self,
        semantic_name: impl Into<String>,
        tried: Vec<String>,
    ) -> bool {
        let semantic_name = semantic_name.into();
        let details = if tried.is_empty() {
            "no configured candidates".to_string()
        } else {
            format!("tried {}", tried.join(", "))
        };
        self.record_issue(
            format!("missing-icon:{semantic_name}"),
            IssueSeverity::Warning,
            format!(
                "missing icon '{semantic_name}' for module '{}': {details}",
                self.module_id
            ),
        )
    }

    pub fn record_optional_missing_icon(
        &self,
        semantic_name: impl Into<String>,
        tried: Vec<String>,
    ) -> bool {
        let semantic_name = semantic_name.into();
        let details = if tried.is_empty() {
            "no configured candidates".to_string()
        } else {
            format!("tried {}", tried.join(", "))
        };
        self.record_issue(
            format!("missing-optional-icon:{semantic_name}"),
            IssueSeverity::Warning,
            format!(
                "missing optional icon '{semantic_name}' for module '{}': {details}",
                self.module_id
            ),
        )
    }

    pub fn record_lifecycle_error(
        &self,
        provider_id: impl Into<String>,
        stage: impl Into<String>,
        message: impl Into<String>,
    ) -> bool {
        let provider_id = provider_id.into();
        let stage = stage.into();
        self.record_issue(
            lifecycle_issue_code(&provider_id, &stage),
            IssueSeverity::Error,
            format!(
                "backend lifecycle '{stage}' failed for provider '{provider_id}': {}",
                message.into()
            ),
        )
    }

    pub fn resolve_lifecycle_error(&self, provider_id: &str, stage: &str) -> bool {
        self.resolve_issue(&lifecycle_issue_code(provider_id, stage))
    }

    pub fn resolve_lifecycle_errors(&self, provider_id: &str) -> usize {
        let prefix = format!("lifecycle:{provider_id}:");
        let codes = self
            .lock_state()
            .issues
            .keys()
            .filter(|code| code.starts_with(&prefix))
            .cloned()
            .collect::<Vec<_>>();
        codes.iter().filter(|code| self.resolve_issue(code)).count()
    }

    pub fn lifecycle_error_records(&self) -> Vec<LifecycleErrorRecord> {
        let state = self.lock_state();
        let mut records = state
            .issues
            .values()
            .filter_map(|issue| {
                let rest = issue.issue_code.strip_prefix("lifecycle:")?;
                let (provider_id, stage) = rest.rsplit_once(':')?;
                Some(LifecycleErrorRecord {
                    provider_id: provider_id.to_string(),
                    stage: stage.to_string(),
                    latest_message: issue.message.clone(),
                    count: issue.count,
                    last_seen: issue.last_seen,
                    active: issue.active,
                })
            })
            .collect::<Vec<_>>();
        records.sort_by(|left, right| {
            left.provider_id
                .cmp(&right.provider_id)
                .then_with(|| left.stage.cmp(&right.stage))
        });
        records
    }

    pub fn health(&self) -> HealthStatus {
        let state = self.lock_state();
        health_from_issues(state.issues.values().filter(|issue| issue.active))
    }

    pub fn active_issues(&self) -> Vec<DiagnosticIssue> {
        self.issues()
            .into_iter()
            .filter(|issue| issue.active)
            .collect()
    }

    pub fn issues(&self) -> Vec<DiagnosticIssue> {
        let mut issues = self.cloned_issues();
        issues.sort_unstable_by(|left, right| left.issue_code.cmp(&right.issue_code));
        issues
    }

    /// Capture one coherent point-in-time view of this module instance.
    /// History, active issues, and health all derive from the same cloned and
    /// once-sorted issue list, so they cannot disagree because an issue changed
    /// between independent lock acquisitions.
    pub fn snapshot(&self) -> DiagnosticInstanceSnapshot {
        let mut issues = self.cloned_issues();
        issues.sort_unstable_by(|left, right| left.issue_code.cmp(&right.issue_code));
        let active_issues = issues
            .iter()
            .filter(|issue| issue.active)
            .cloned()
            .collect();
        let health = health_from_sorted_issues(&issues);
        DiagnosticInstanceSnapshot {
            instance_id: self.instance_id.clone(),
            health,
            issues,
            active_issues,
        }
    }

    pub fn error_count(&self) -> u64 {
        self.lock_state().error_count
    }
}

fn lifecycle_issue_code(provider_id: &str, stage: &str) -> String {
    format!("lifecycle:{provider_id}:{stage}")
}

fn health_from_issues<'a>(issues: impl Iterator<Item = &'a DiagnosticIssue>) -> HealthStatus {
    let mut issues = issues.collect::<Vec<_>>();
    issues.sort_by(|left, right| {
        right
            .severity
            .cmp(&left.severity)
            .then_with(|| left.issue_code.cmp(&right.issue_code))
    });
    let Some(first) = issues.first() else {
        return HealthStatus::Healthy;
    };
    let details = issues
        .iter()
        .map(|issue| format!("{}: {}", issue.issue_code, issue.message))
        .collect::<Vec<_>>()
        .join("; ");
    match first.severity {
        IssueSeverity::Warning => HealthStatus::Degraded(details),
        IssueSeverity::Error => HealthStatus::Error(details),
    }
}

fn health_from_sorted_issues(issues: &[DiagnosticIssue]) -> HealthStatus {
    let has_error = issues
        .iter()
        .any(|issue| issue.active && issue.severity == IssueSeverity::Error);
    let severity = if has_error {
        IssueSeverity::Error
    } else if issues.iter().any(|issue| issue.active) {
        IssueSeverity::Warning
    } else {
        return HealthStatus::Healthy;
    };

    let mut details = issues
        .iter()
        .filter(|issue| issue.active && issue.severity == severity)
        .map(|issue| format!("{}: {}", issue.issue_code, issue.message))
        .collect::<Vec<_>>();
    if has_error {
        details.extend(
            issues
                .iter()
                .filter(|issue| issue.active && issue.severity == IssueSeverity::Warning)
                .map(|issue| format!("{}: {}", issue.issue_code, issue.message)),
        );
    }
    let details = details.join("; ");
    match severity {
        IssueSeverity::Warning => HealthStatus::Degraded(details),
        IssueSeverity::Error => HealthStatus::Error(details),
    }
}

#[derive(Debug)]
pub struct DiagnosticsCollector {
    modules: BTreeMap<(String, String), Diagnostics>,
    generation: Arc<AtomicU64>,
}

impl Default for DiagnosticsCollector {
    fn default() -> Self {
        Self {
            modules: BTreeMap::new(),
            generation: Arc::new(AtomicU64::new(0)),
        }
    }
}

impl DiagnosticsCollector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Monotonic revision of all registered diagnostic state. Runtime
    /// snapshots use this as a cheap cache key instead of rebuilding the
    /// complete issue projection just to discover whether it changed.
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }

    /// Registering an existing identity replaces its old issue set. This
    /// makes remount/profile activation explicit and prevents stale rows from
    /// accumulating across generations.
    pub fn register(&mut self, module_id: impl Into<String>) -> Diagnostics {
        self.register_instance(module_id, "")
    }

    pub fn register_instance(
        &mut self,
        module_id: impl Into<String>,
        instance_id: impl Into<String>,
    ) -> Diagnostics {
        let module_id = module_id.into();
        let instance_id = instance_id.into();
        let diagnostics = Diagnostics::new_instance_with_generation(
            module_id.clone(),
            instance_id.clone(),
            Arc::clone(&self.generation),
        );
        self.modules
            .insert((module_id, instance_id), diagnostics.clone());
        self.generation.fetch_add(1, Ordering::Relaxed);
        diagnostics
    }

    pub fn unregister(&mut self, module_id: &str, instance_id: &str) -> bool {
        let removed = self
            .modules
            .remove(&(module_id.to_string(), instance_id.to_string()))
            .is_some();
        if removed {
            self.generation.fetch_add(1, Ordering::Relaxed);
        }
        removed
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
            .get(&(provider_id.clone(), String::new()))
            .cloned()
            .or_else(|| {
                self.modules
                    .iter()
                    .find(|((module_id, _), _)| module_id == &provider_id)
                    .map(|(_, diagnostics)| diagnostics.clone())
            })
            .unwrap_or_else(|| self.register_instance(provider_id.clone(), ""));
        diagnostics.record_lifecycle_error(provider_id, stage, message)
    }

    /// Record a shell-supervised component/runtime failure against the exact
    /// mounted instance. Unlike the legacy lifecycle helper this keeps the
    /// diagnostic category and wording about component execution rather than
    /// implying that a backend provider failed.
    pub fn record_component_runtime_error(
        &mut self,
        module_id: impl Into<String>,
        instance_id: impl Into<String>,
        phase: impl Into<String>,
        message: impl Into<String>,
    ) -> bool {
        let module_id = module_id.into();
        let instance_id = instance_id.into();
        let phase = phase.into();
        let diagnostics = self
            .modules
            .get(&(module_id.clone(), instance_id.clone()))
            .cloned()
            .unwrap_or_else(|| self.register_instance(module_id.clone(), instance_id.clone()));
        diagnostics.record_issue_with_source(
            format!("component-runtime:{module_id}:{instance_id}:{phase}"),
            DiagnosticCategory::Runtime,
            IssueSeverity::Error,
            format!(
                "component runtime '{phase}' failed for module '{module_id}' instance '{instance_id}': {}; the failed work was contained",
                message.into()
            ),
            None,
            None,
        )
    }

    /// Record a typed issue for a module-owned instance, creating the
    /// diagnostic identity only when the first issue is observed. This keeps
    /// shell-generated metadata diagnostics visible even before a module has a
    /// mounted runtime.
    pub fn record_instance_issue_with_source(
        &mut self,
        module_id: impl Into<String>,
        instance_id: impl Into<String>,
        issue_code: impl Into<String>,
        category: DiagnosticCategory,
        severity: IssueSeverity,
        message: impl Into<String>,
        source_path: Option<String>,
        source_span: Option<DiagnosticSourceSpan>,
    ) -> bool {
        let module_id = module_id.into();
        let instance_id = instance_id.into();
        let diagnostics = self
            .modules
            .get(&(module_id.clone(), instance_id.clone()))
            .cloned()
            .unwrap_or_else(|| self.register_instance(module_id, instance_id));
        diagnostics.record_issue_with_source(
            issue_code,
            category,
            severity,
            message,
            source_path,
            source_span,
        )
    }

    /// Resolve one issue for one exact module instance without hiding
    /// independent issues owned by that instance.
    pub fn resolve_instance_issue(
        &self,
        module_id: &str,
        instance_id: &str,
        issue_code: &str,
    ) -> bool {
        self.modules
            .get(&(module_id.to_string(), instance_id.to_string()))
            .is_some_and(|diagnostics| diagnostics.resolve_issue(issue_code))
    }

    pub fn resolve_lifecycle_errors(&self, provider_id: &str) -> usize {
        self.modules
            .iter()
            .filter(|((module_id, _), _)| module_id == provider_id)
            .map(|(_, diagnostics)| diagnostics.resolve_lifecycle_errors(provider_id))
            .sum()
    }

    /// Resolve one lifecycle stage without clearing unrelated failures owned by
    /// the same module. Supervisors that publish independent health domains
    /// (for example the shell's file watcher) use this for recovery.
    pub fn resolve_lifecycle_error(&self, provider_id: &str, stage: &str) -> bool {
        self.modules
            .iter()
            .filter(|((module_id, _), _)| module_id == provider_id)
            .any(|(_, diagnostics)| diagnostics.resolve_lifecycle_error(provider_id, stage))
    }

    /// Return deterministic module aggregates with instance issue detail.
    pub fn snapshot(&self) -> Vec<DiagnosticModuleSnapshot> {
        let mut modules = BTreeMap::<String, Vec<DiagnosticInstanceSnapshot>>::new();
        for ((module_id, _), diagnostics) in &self.modules {
            modules
                .entry(module_id.clone())
                .or_default()
                .push(diagnostics.snapshot());
        }

        modules
            .into_iter()
            .map(|(module_id, instances)| {
                let health = health_from_issues(
                    instances
                        .iter()
                        .flat_map(|instance| instance.active_issues.iter()),
                );
                DiagnosticModuleSnapshot {
                    module_id,
                    health,
                    instances,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handler_errors_are_deduplicated_by_stable_issue_code() {
        let diagnostics = Diagnostics::new("@test/frontend");

        assert!(diagnostics.record_handler_error("@test/frontend", "onChange", "boom"));
        assert!(!diagnostics.record_handler_error("@test/frontend", "onChange", "changed"));
        assert!(diagnostics.record_handler_error("@test/frontend", "onRelease", "boom"));

        assert_eq!(diagnostics.error_count(), 2);
        assert_eq!(diagnostics.active_issues().len(), 2);
        assert!(matches!(diagnostics.health(), HealthStatus::Error(_)));
        assert_eq!(diagnostics.issues()[0].category, DiagnosticCategory::Script);
    }

    #[test]
    fn structured_issue_metadata_survives_deduplication_and_updates() {
        let diagnostics = Diagnostics::new("@test/frontend");
        let span = DiagnosticSourceSpan::new(12, 18);

        assert!(diagnostics.record_issue_with_source(
            "component-parse",
            DiagnosticCategory::Template,
            IssueSeverity::Error,
            "first",
            Some("/tmp/main.mesh".into()),
            Some(span),
        ));
        assert!(!diagnostics.record_issue_with_source(
            "component-parse",
            DiagnosticCategory::Style,
            IssueSeverity::Error,
            "updated",
            Some("/tmp/main.mesh".into()),
            Some(DiagnosticSourceSpan::new(20, 24)),
        ));

        let issue = &diagnostics.issues()[0];
        assert_eq!(issue.category, DiagnosticCategory::Style);
        assert_eq!(issue.source_path.as_deref(), Some("/tmp/main.mesh"));
        assert_eq!(issue.source_span, Some(DiagnosticSourceSpan::new(20, 24)));
        assert_eq!(issue.message, "updated");
    }

    #[test]
    fn missing_icon_diagnostics_are_deduplicated_and_aggregate() {
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
    fn lifecycle_errors_are_keyed_by_provider_and_stage() {
        let diagnostics = Diagnostics::new("@mesh/pipewire-audio");

        assert!(diagnostics.record_lifecycle_error("@mesh/pipewire-audio", "poll", "boom 1"));
        assert!(!diagnostics.record_lifecycle_error("@mesh/pipewire-audio", "poll", "boom 2"));
        assert!(diagnostics.record_lifecycle_error("@mesh/pipewire-audio", "init", "boom"));

        assert_eq!(diagnostics.error_count(), 2);
        let records = diagnostics.lifecycle_error_records();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].count, 1);
        assert_eq!(records[1].count, 2);
        assert!(records.iter().all(|record| record.active));
    }

    #[test]
    fn resolving_an_issue_restores_health_without_erasing_history() {
        let diagnostics = Diagnostics::new("@test/backend");

        diagnostics.record_issue("fatal", IssueSeverity::Error, "unrecoverable");
        diagnostics.degraded("later warning");
        diagnostics.healthy();
        assert!(matches!(diagnostics.health(), HealthStatus::Error(_)));

        diagnostics.resolve_issue("runtime");
        assert!(matches!(diagnostics.health(), HealthStatus::Error(_)));
        diagnostics.resolve_issue("fatal");
        assert!(matches!(diagnostics.health(), HealthStatus::Healthy));
        assert_eq!(diagnostics.active_issues().len(), 0);
        assert_eq!(diagnostics.error_count(), 1);
    }

    #[test]
    fn a_warning_cannot_downgrade_an_active_error() {
        let diagnostics = Diagnostics::new("@test/module");

        diagnostics.error("hard failure");
        diagnostics.degraded("soft follow-up");

        assert!(matches!(diagnostics.health(), HealthStatus::Error(_)));
        assert_eq!(diagnostics.error_count(), 1);
    }

    #[test]
    fn collector_replaces_and_unregisters_instance_registrations_deterministically() {
        let mut collector = DiagnosticsCollector::new();
        let first = collector.register_instance("@test/module", "z");
        first.degraded("old");
        collector.register_instance("@test/module", "z");
        let second = collector.register_instance("@test/module", "a");
        second.error("new");

        let snapshot = collector.snapshot();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].module_id, "@test/module");
        assert!(matches!(snapshot[0].health, HealthStatus::Error(_)));
        assert_eq!(
            snapshot[0]
                .instances
                .iter()
                .map(|instance| instance.instance_id.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "z"]
        );
        assert!(snapshot[0].instances[1].active_issues.is_empty());

        assert!(collector.unregister("@test/module", "a"));
        assert!(!collector.unregister("@test/module", "a"));
        assert_eq!(collector.snapshot()[0].instances.len(), 1);
    }

    #[test]
    fn instance_snapshot_keeps_history_active_issues_and_health_coherent() {
        let diagnostics = Diagnostics::new_instance("@test/module", "instance");
        diagnostics.record_issue("warning", IssueSeverity::Warning, "warning message");
        diagnostics.record_issue("error", IssueSeverity::Error, "error message");

        let all_active_snapshot = diagnostics.snapshot();
        assert!(matches!(
            all_active_snapshot.health,
            HealthStatus::Error(message)
                if message == "error: error message; warning: warning message"
        ));
        diagnostics.resolve_issue("warning");

        diagnostics.reset_lock_acquisitions();
        let snapshot = diagnostics.snapshot();

        assert_eq!(diagnostics.lock_acquisition_count(), 1);
        assert_eq!(
            snapshot
                .issues
                .iter()
                .map(|issue| issue.issue_code.as_str())
                .collect::<Vec<_>>(),
            vec!["error", "warning"]
        );
        assert_eq!(
            snapshot
                .active_issues
                .iter()
                .map(|issue| issue.issue_code.as_str())
                .collect::<Vec<_>>(),
            vec!["error"]
        );
        assert!(matches!(snapshot.health, HealthStatus::Error(message) if
            message == "error: error message"));
    }

    // cargo test -p mesh-core-diagnostics --release -- diagnostics_snapshot_release_benchmark --ignored --nocapture
    #[test]
    #[ignore = "release-only diagnostics snapshot lock/allocation benchmark"]
    fn diagnostics_snapshot_release_benchmark() {
        for &issue_count in &[0, 10, 100] {
            let (collector, diagnostics) = benchmark_collector(issue_count);
            let legacy = measure_snapshot(&diagnostics, || legacy_collector_snapshot(&diagnostics));
            let optimized = measure_snapshot(&diagnostics, || collector.snapshot());
            let legacy_payload = snapshot_payload(&legacy.snapshot);
            let optimized_payload = snapshot_payload(&optimized.snapshot);

            assert_eq!(legacy_payload, optimized_payload);
            assert_eq!(legacy.lock_acquisitions, diagnostics.len() as u64 * 3 * 100);
            assert_eq!(optimized.lock_acquisitions, diagnostics.len() as u64 * 100);
            eprintln!(
                "diagnostics snapshot: modules=128 instances_per_module=4 issues={issue_count} legacy_ns={}/{}, optimized_ns={}/{} speedup={:.2}x locks={}/{} allocations={}/{} allocated_bytes={}/{} payload_bytes={}",
                legacy.median_ns,
                legacy.p95_ns,
                optimized.median_ns,
                optimized.p95_ns,
                legacy.median_ns as f64 / optimized.median_ns.max(1) as f64,
                legacy.lock_acquisitions,
                optimized.lock_acquisitions,
                legacy.allocation_count,
                optimized.allocation_count,
                legacy.allocated_bytes,
                optimized.allocated_bytes,
                serde_json::to_vec(&optimized_payload)
                    .expect("diagnostics payload should serialize")
                    .len(),
            );
        }
    }

    struct SnapshotBenchmarkResult {
        snapshot: Vec<DiagnosticModuleSnapshot>,
        median_ns: u128,
        p95_ns: u128,
        lock_acquisitions: u64,
        allocation_count: u64,
        allocated_bytes: u64,
    }

    fn benchmark_collector(issue_count: usize) -> (DiagnosticsCollector, Vec<Diagnostics>) {
        let mut collector = DiagnosticsCollector::new();
        let mut diagnostics = Vec::with_capacity(128 * 4);
        for module_index in 0..128 {
            for instance_index in 0..4 {
                let module_id = format!("@bench/module-{module_index}");
                let instance_id = format!("instance-{instance_index}");
                let instance = collector.register_instance(&module_id, &instance_id);
                for issue_index in 0..issue_count {
                    instance.record_issue(
                        format!("issue-{issue_index:03}"),
                        IssueSeverity::Warning,
                        format!("benchmark issue {issue_index}"),
                    );
                }
                diagnostics.push(instance);
            }
        }
        (collector, diagnostics)
    }

    fn measure_snapshot(
        diagnostics: &[Diagnostics],
        mut operation: impl FnMut() -> Vec<DiagnosticModuleSnapshot>,
    ) -> SnapshotBenchmarkResult {
        const ITERATIONS: usize = 100;

        for instance in diagnostics {
            instance.reset_lock_acquisitions();
        }
        test_alloc::reset();
        let allocation_before = test_alloc::snapshot();
        let mut samples = Vec::with_capacity(ITERATIONS);
        let mut last_snapshot = None;
        for _ in 0..ITERATIONS {
            let started = std::time::Instant::now();
            let snapshot = operation();
            samples.push(started.elapsed().as_nanos());
            std::hint::black_box(&snapshot);
            last_snapshot = Some(snapshot);
        }
        samples.sort_unstable();
        let allocation_after = test_alloc::snapshot();
        let snapshot = last_snapshot.expect("benchmark should produce a snapshot");
        SnapshotBenchmarkResult {
            snapshot,
            median_ns: samples[samples.len() / 2],
            p95_ns: samples[(samples.len() * 95) / 100],
            lock_acquisitions: diagnostics
                .iter()
                .map(Diagnostics::lock_acquisition_count)
                .sum(),
            allocation_count: allocation_after.0.saturating_sub(allocation_before.0),
            allocated_bytes: allocation_after.1.saturating_sub(allocation_before.1),
        }
    }

    fn legacy_collector_snapshot(diagnostics: &[Diagnostics]) -> Vec<DiagnosticModuleSnapshot> {
        let mut modules = BTreeMap::<String, Vec<DiagnosticInstanceSnapshot>>::new();
        for diagnostics in diagnostics {
            let health = {
                let state = diagnostics.lock_state();
                health_from_issues(state.issues.values().filter(|issue| issue.active))
            };
            let issues = diagnostics.issues();
            let active_issues = diagnostics
                .issues()
                .into_iter()
                .filter(|issue| issue.active)
                .collect();
            modules
                .entry(diagnostics.module_id().to_string())
                .or_default()
                .push(DiagnosticInstanceSnapshot {
                    instance_id: diagnostics.instance_id().to_string(),
                    health,
                    issues,
                    active_issues,
                });
        }

        modules
            .into_iter()
            .map(|(module_id, instances)| {
                let health = health_from_issues(
                    instances
                        .iter()
                        .flat_map(|instance| instance.active_issues.iter()),
                );
                DiagnosticModuleSnapshot {
                    module_id,
                    health,
                    instances,
                }
            })
            .collect()
    }

    fn snapshot_payload(snapshot: &[DiagnosticModuleSnapshot]) -> serde_json::Value {
        serde_json::json!({
            "modules": snapshot.iter().map(|module| {
                serde_json::json!({
                    "module_id": module.module_id,
                    "health": module.health.to_string(),
                    "instances": module.instances.iter().map(|instance| {
                        serde_json::json!({
                            "instance_id": instance.instance_id,
                            "health": instance.health.to_string(),
                            "issues": instance.issues.iter().map(issue_payload).collect::<Vec<_>>(),
                            "active_issues": instance.active_issues.iter().map(issue_payload).collect::<Vec<_>>(),
                        })
                    }).collect::<Vec<_>>(),
                })
            }).collect::<Vec<_>>(),
        })
    }

    fn issue_payload(issue: &DiagnosticIssue) -> serde_json::Value {
        serde_json::json!({
            "module_id": issue.module_id,
            "instance_id": issue.instance_id,
            "issue_code": issue.issue_code,
            "category": issue.category.as_str(),
            "severity": format!("{:?}", issue.severity).to_lowercase(),
            "message": issue.message,
            "source_path": issue.source_path,
            "source_span": issue.source_span.map(|span| serde_json::json!({
                "start": span.start,
                "end": span.end,
            })),
            "first_seen": format!("{:?}", issue.first_seen),
            "last_seen": format!("{:?}", issue.last_seen),
            "count": issue.count,
            "active": issue.active,
        })
    }
}
