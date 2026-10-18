/// Shell sound event dispatch.
///
/// Core maps semantic shell events to a typed service request. Normal interface
/// routing selects and invokes the active provider; the module does playback.
use mesh_core_capability::{CapabilityCatalog, CapabilitySet};
use mesh_core_config::ShellSounds;

pub(super) enum SoundKind {
    Startup,
}

/// A core-triggered call of a declared service method, dispatched like any
/// other caller's.
pub(super) struct ShellSoundCall {
    pub(super) interface: &'static str,
    pub(super) command: &'static str,
    pub(super) payload: serde_json::Value,
    pub(super) source_module_id: &'static str,
    pub(super) source_capabilities: CapabilitySet,
}

/// Build the generic interface call for a configured shell sound.
pub(super) fn shell_sound_request(kind: SoundKind, sounds: &ShellSounds) -> Option<ShellSoundCall> {
    let path = match kind {
        SoundKind::Startup => sounds.startup.as_deref(),
    };

    let path = path?;
    let source_capabilities = CapabilityCatalog::builtin()
        .capability_set(["service.audio.control"])
        .expect("built-in sound routing capability must remain in the catalog");

    Some(ShellSoundCall {
        interface: "mesh.audio",
        command: "play_sound",
        payload: serde_json::json!({ "path": path }),
        source_module_id: "@mesh/shell",
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
        let ShellSoundCall {
            interface,
            command,
            payload,
            source_module_id,
            source_capabilities,
        } = request;

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
