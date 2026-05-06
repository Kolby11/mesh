#[derive(Debug, thiserror::Error)]
pub enum BackendScriptError {
    #[error("script error in {module_id}: {message}")]
    Runtime { module_id: String, message: String },

    #[error("backend script {module_id} is missing required entrypoint {name}()")]
    MissingEntrypoint { module_id: String, name: String },

    /// State snapshot or emit serialization failed — exported state could not be converted to JSON.
    #[error("backend script {module_id} failed to export state snapshot: {message}")]
    SnapshotFailed { module_id: String, message: String },

    /// Command result returned by the handler could not be converted to JSON.
    #[error("backend script {module_id} failed to convert command result: {message}")]
    CommandResultConversionFailed { module_id: String, message: String },
}
