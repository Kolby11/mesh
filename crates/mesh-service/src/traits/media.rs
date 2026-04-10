use serde::{Deserialize, Serialize};
use std::future::Future;

/// Current playback state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlaybackState {
    Playing,
    Paused,
    Stopped,
}

/// Info about the currently playing media.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaInfo {
    pub player_id: String,
    pub player_name: String,
    pub state: PlaybackState,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub art_url: Option<String>,
    pub position: Option<u64>,
    pub duration: Option<u64>,
}

/// Events emitted by the media backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MediaEvent {
    PlayerAdded(MediaInfo),
    PlayerRemoved(String),
    PlaybackChanged(MediaInfo),
    MetadataChanged(MediaInfo),
}

/// The media service trait.
///
/// Backends implement this for specific media systems (MPRIS, custom player daemons).
pub trait MediaService: Send + Sync {
    fn backend_id(&self) -> &str;

    fn players(&self) -> impl Future<Output = Result<Vec<MediaInfo>, MediaError>> + Send;
    fn active_player(&self) -> impl Future<Output = Result<Option<MediaInfo>, MediaError>> + Send;

    fn play(&self, player_id: &str) -> impl Future<Output = Result<(), MediaError>> + Send;
    fn pause(&self, player_id: &str) -> impl Future<Output = Result<(), MediaError>> + Send;
    fn next(&self, player_id: &str) -> impl Future<Output = Result<(), MediaError>> + Send;
    fn previous(&self, player_id: &str) -> impl Future<Output = Result<(), MediaError>> + Send;

    fn subscribe(&self) -> impl Future<Output = Result<tokio::sync::broadcast::Receiver<MediaEvent>, MediaError>> + Send;
}

#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("player not found: {0}")]
    PlayerNotFound(String),

    #[error("backend unavailable: {0}")]
    Unavailable(String),

    #[error("{0}")]
    Other(String),
}
