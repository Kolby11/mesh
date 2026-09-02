#![allow(dead_code)] // Sound variants await their corresponding module providers.

use super::types::CoreRequest;
/// Shell sound event dispatch.
///
/// Core maps semantic shell events to a typed service request. Normal interface
/// routing selects and invokes the active provider; the module does playback.
use mesh_core_capability::CapabilityCatalog;
use mesh_core_config::ShellSounds;

pub(super) enum SoundKind {
    Startup,
    Shutdown,
    DeviceConnected,
    DeviceDisconnected,
    Error,
    Notification,
}

/// Build the generic interface request for a configured shell sound.
pub(super) fn shell_sound_request(kind: SoundKind, sounds: &ShellSounds) -> Option<CoreRequest> {
    let path = match kind {
        SoundKind::Startup => sounds.startup.as_deref(),
        SoundKind::Shutdown => sounds.shutdown.as_deref(),
        SoundKind::DeviceConnected => sounds.device_connected.as_deref(),
        SoundKind::DeviceDisconnected => sounds.device_disconnected.as_deref(),
        SoundKind::Error => sounds.error.as_deref(),
        SoundKind::Notification => sounds.notification.as_deref(),
    };

    let path = path?;
    let source_capabilities = CapabilityCatalog::builtin()
        .capability_set(["service.audio.control"])
        .expect("built-in sound routing capability must remain in the catalog");

    Some(CoreRequest::ServiceCommand {
        interface: "mesh.audio".to_string(),
        command: "play_sound".to_string(),
        payload: serde_json::json!({ "path": path }),
        source_module_id: "@mesh/shell".to_string(),
        source_capabilities,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core_capability::Capability;

    #[test]
    fn configured_sound_becomes_a_generic_service_request() {
        let sounds = ShellSounds {
            startup: Some("sounds/startup.wav".to_string()),
            ..ShellSounds::default()
        };

        let request = shell_sound_request(SoundKind::Startup, &sounds)
            .expect("configured startup sound should produce a request");
        let CoreRequest::ServiceCommand {
            interface,
            command,
            payload,
            source_module_id,
            source_capabilities,
        } = request
        else {
            panic!("shell sound must use generic service routing");
        };

        assert_eq!(interface, "mesh.audio");
        assert_eq!(command, "play_sound");
        assert_eq!(payload, serde_json::json!({ "path": "sounds/startup.wav" }));
        assert_eq!(source_module_id, "@mesh/shell");
        assert!(source_capabilities.is_granted(&Capability::new("service.audio.control")));
    }

    #[test]
    fn unconfigured_sound_produces_no_request() {
        assert!(shell_sound_request(SoundKind::Startup, &ShellSounds::default()).is_none());
    }
}
