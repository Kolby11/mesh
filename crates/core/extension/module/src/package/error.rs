use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleManifestDiagnosticSeverity {
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleManifestDiagnostic {
    pub severity: ModuleManifestDiagnosticSeverity,
    pub path: PathBuf,
    pub module_id: Option<String>,
    pub field_path: Option<String>,
    pub message: String,
    pub suggested_action: String,
}

impl ModuleManifestDiagnostic {
    pub fn warning(
        path: impl Into<PathBuf>,
        module_id: Option<String>,
        field_path: Option<String>,
        message: impl Into<String>,
        suggested_action: impl Into<String>,
    ) -> Self {
        Self {
            severity: ModuleManifestDiagnosticSeverity::Warning,
            path: path.into(),
            module_id,
            field_path,
            message: message.into(),
            suggested_action: suggested_action.into(),
        }
    }

    pub fn error(
        path: impl Into<PathBuf>,
        module_id: Option<String>,
        field_path: Option<String>,
        message: impl Into<String>,
        suggested_action: impl Into<String>,
    ) -> Self {
        Self {
            severity: ModuleManifestDiagnosticSeverity::Error,
            path: path.into(),
            module_id,
            field_path,
            message: message.into(),
            suggested_action: suggested_action.into(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ModuleManifestError {
    #[error("failed to read module manifest {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to parse module manifest {path}: {source}")]
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[error("invalid MESH_HOME: {0}")]
    InvalidMeshHome(String),

    #[error("invalid module manifest: {0}")]
    Validation(String),

    #[error("legacy manifest error for {path}: {message}")]
    LegacyManifest { path: PathBuf, message: String },

    #[error("module manifest diagnostic: {diagnostic:?}")]
    Diagnostic {
        diagnostic: ModuleManifestDiagnostic,
    },
}
