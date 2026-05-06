use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum PackageManifestError {
    #[error("failed to read package manifest {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to parse package manifest {path}: {source}")]
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[error("invalid MESH_HOME: {0}")]
    InvalidMeshHome(String),

    #[error("invalid package manifest: {0}")]
    Validation(String),

    #[error("legacy manifest error for {path}: {message}")]
    LegacyManifest { path: PathBuf, message: String },
}
