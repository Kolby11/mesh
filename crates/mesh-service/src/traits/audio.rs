use serde::{Deserialize, Serialize};
use std::future::Future;

/// Unique identifier for an audio device or stream.
pub type AudioId = String;

/// An audio output or input device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioDevice {
    pub id: AudioId,
    pub name: String,
    pub description: String,
    pub is_default: bool,
    pub volume: f64,
    pub muted: bool,
}

/// An active audio stream (application audio).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioStream {
    pub id: AudioId,
    pub name: String,
    pub application: Option<String>,
    pub volume: f64,
    pub muted: bool,
}

/// Events emitted by the audio backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AudioEvent {
    DeviceAdded(AudioDevice),
    DeviceRemoved(AudioId),
    DeviceChanged(AudioDevice),
    StreamAdded(AudioStream),
    StreamRemoved(AudioId),
    StreamChanged(AudioStream),
    DefaultChanged(AudioId),
}

/// The audio service trait.
///
/// Backends implement this for specific audio systems (PipeWire, PulseAudio, ALSA).
/// Frontends consume this through the service registry — they never know which backend is active.
pub trait AudioService: Send + Sync {
    /// Return the service identifier (e.g. "pipewire", "pulseaudio").
    fn backend_id(&self) -> &str;

    /// List all output devices.
    fn output_devices(&self) -> impl Future<Output = Result<Vec<AudioDevice>, AudioError>> + Send;

    /// List all input devices.
    fn input_devices(&self) -> impl Future<Output = Result<Vec<AudioDevice>, AudioError>> + Send;

    /// List active audio streams.
    fn streams(&self) -> impl Future<Output = Result<Vec<AudioStream>, AudioError>> + Send;

    /// Get the default output device.
    fn default_output(
        &self,
    ) -> impl Future<Output = Result<Option<AudioDevice>, AudioError>> + Send;

    /// Set volume on a device (0.0 to 1.0).
    fn set_volume(
        &self,
        device_id: &str,
        volume: f64,
    ) -> impl Future<Output = Result<(), AudioError>> + Send;

    /// Mute or unmute a device.
    fn set_muted(
        &self,
        device_id: &str,
        muted: bool,
    ) -> impl Future<Output = Result<(), AudioError>> + Send;

    /// Set the default output device.
    fn set_default_output(
        &self,
        device_id: &str,
    ) -> impl Future<Output = Result<(), AudioError>> + Send;

    /// Subscribe to audio events. Returns a receiver.
    fn subscribe(
        &self,
    ) -> impl Future<Output = Result<tokio::sync::broadcast::Receiver<AudioEvent>, AudioError>> + Send;
}

#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("device not found: {0}")]
    DeviceNotFound(String),

    #[error("backend unavailable: {0}")]
    Unavailable(String),

    #[error("{0}")]
    Other(String),
}
