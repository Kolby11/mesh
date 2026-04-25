/// Shell sound event dispatch.
///
/// Core's only audio responsibility: when a system event occurs (startup,
/// device connected, etc.), send a `play-sound` command to whatever audio
/// backend plugin is registered. The plugin does the actual playback.
use mesh_config::ShellSounds;
use tokio::sync::mpsc;
use super::types::ServiceCommandMsg;

pub(super) enum SoundKind {
    Startup,
    Shutdown,
    DeviceConnected,
    DeviceDisconnected,
    Error,
    Notification,
}

/// Send a `play-sound` command to the audio backend if a sound is configured
/// for this event kind and a handler is registered.
pub(super) fn play_shell_sound(
    kind: SoundKind,
    sounds: &ShellSounds,
    audio_handler: Option<&mpsc::UnboundedSender<ServiceCommandMsg>>,
) {
    let path = match kind {
        SoundKind::Startup => sounds.startup.as_deref(),
        SoundKind::Shutdown => sounds.shutdown.as_deref(),
        SoundKind::DeviceConnected => sounds.device_connected.as_deref(),
        SoundKind::DeviceDisconnected => sounds.device_disconnected.as_deref(),
        SoundKind::Error => sounds.error.as_deref(),
        SoundKind::Notification => sounds.notification.as_deref(),
    };

    let Some(path) = path else { return };
    let Some(handler) = audio_handler else { return };

    let _ = handler.send(ServiceCommandMsg {
        command: "play-sound".to_string(),
        payload: serde_json::json!({ "path": path }),
    });
}
