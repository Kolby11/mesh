use serde::{Deserialize, Serialize};
use std::future::Future;

/// Events emitted by the brightness backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BrightnessEvent {
    Changed(f64),
}

/// The brightness service trait.
///
/// Backends implement this for specific brightness control (sysfs, DDC, etc.).
pub trait BrightnessService: Send + Sync {
    fn backend_id(&self) -> &str;

    /// Get current brightness (0.0 to 1.0).
    fn get(&self) -> impl Future<Output = Result<f64, BrightnessError>> + Send;

    /// Set brightness (0.0 to 1.0).
    fn set(&self, value: f64) -> impl Future<Output = Result<(), BrightnessError>> + Send;

    fn subscribe(
        &self,
    ) -> impl Future<
        Output = Result<tokio::sync::broadcast::Receiver<BrightnessEvent>, BrightnessError>,
    > + Send;
}

#[derive(Debug, thiserror::Error)]
pub enum BrightnessError {
    #[error("backend unavailable: {0}")]
    Unavailable(String),

    #[error("{0}")]
    Other(String),
}
