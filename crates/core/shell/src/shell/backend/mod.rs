use super::*;

mod candidates;
mod lifecycle;
mod spawn;
mod supervision;

#[cfg(test)]
pub(in crate::shell) use candidates::backend_launch_candidates_from_graph;
pub(in crate::shell) use candidates::launch_candidate_for_provider;
pub(in crate::shell) use supervision::{BackendRespawnContext, BackendSupervisionState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::shell) enum BackendRuntimeStatus {
    NoActiveProvider,
    UnmetBackendRequirement,
    OptionalBackendUnavailable,
    OptionalBackendInactive,
    InvalidManifest,
    MissingCapability,
    MissingEntrypoint,
    MissingBinary,
    InitFailed,
    Running,
    PollFailed,
    Failed,
    Stopped,
    /// Benched by the supervisor for this session after exhausting restarts.
    Quarantined,
}

impl BackendRuntimeStatus {
    pub(in crate::shell) fn as_str(self) -> &'static str {
        match self {
            Self::NoActiveProvider => "no_active_provider",
            Self::UnmetBackendRequirement => "unmet_backend_requirement",
            Self::OptionalBackendUnavailable => "optional_backend_unavailable",
            Self::OptionalBackendInactive => "optional_backend_inactive",
            Self::InvalidManifest => "invalid_manifest",
            Self::MissingCapability => "missing_capability",
            Self::MissingEntrypoint => "missing_entrypoint",
            Self::MissingBinary => "missing_binary",
            Self::InitFailed => "init_failed",
            Self::Running => "running",
            Self::PollFailed => "poll_failed",
            Self::Failed => "failed",
            Self::Stopped => "stopped",
            Self::Quarantined => "quarantined",
        }
    }

    pub(in crate::shell) fn from_str(status: &str) -> Self {
        match status {
            "no_active_provider" => Self::NoActiveProvider,
            "unmet_backend_requirement" => Self::UnmetBackendRequirement,
            "optional_backend_unavailable" => Self::OptionalBackendUnavailable,
            "optional_backend_inactive" => Self::OptionalBackendInactive,
            "invalid_manifest" => Self::InvalidManifest,
            "missing_capability" => Self::MissingCapability,
            "missing_entrypoint" => Self::MissingEntrypoint,
            "missing_binary" => Self::MissingBinary,
            "init_failed" => Self::InitFailed,
            "running" => Self::Running,
            "poll_failed" => Self::PollFailed,
            "stopped" => Self::Stopped,
            "quarantined" => Self::Quarantined,
            _ => Self::Failed,
        }
    }
}

#[derive(Debug, Clone)]
pub(in crate::shell) struct BackendRuntimeStatusEntry {
    pub(in crate::shell) interface: String,
    pub(in crate::shell) provider_id: String,
    pub(in crate::shell) status: BackendRuntimeStatus,
    pub(in crate::shell) message: String,
    /// Cumulative number of failure-category status updates recorded for this entry.
    pub(in crate::shell) failure_count: u64,
}

#[derive(Debug, Clone)]
pub(in crate::shell) struct BackendLaunchCandidate {
    pub(in crate::shell) module_id: String,
    pub(in crate::shell) interface: String,
    pub(in crate::shell) service_name: String,
    pub(in crate::shell) entrypoint_path: PathBuf,
    pub(in crate::shell) script_source: String,
    pub(in crate::shell) capabilities: Vec<String>,
    pub(in crate::shell) settings: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct BackendLifecycleStatusRecord {
    pub(super) interface: String,
    pub(super) provider_id: Option<String>,
    pub(super) status: &'static str,
    pub(super) message: String,
}
