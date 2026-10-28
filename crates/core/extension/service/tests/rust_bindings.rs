//! The Rust provider bindings compile and dispatch as the contract declares.
//!
//! `generated/audio.rs` is the generator's output for
//! `fixtures/audio.contract.json`, checked in so this test compiles it.
//! Regenerate it with `MESH_BLESS=1 cargo test -p mesh-core-service --test rust_bindings`.

use serde_json::json;
use std::path::PathBuf;

#[allow(dead_code)]
mod audio {
    include!("generated/audio.rs");
}

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests")
}

#[test]
fn checked_in_bindings_match_the_generator() {
    let declaration: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(fixture_dir().join("fixtures/audio.contract.json")).unwrap(),
    )
    .unwrap();
    let contract =
        mesh_core_service::parse_compiled_contract("mesh.audio", "1.2", &declaration).unwrap();
    let generated = mesh_core_service::generate_rust_bindings(&contract).unwrap();
    let path = fixture_dir().join("generated/audio.rs");
    if std::env::var_os("MESH_BLESS").is_some() {
        std::fs::write(&path, &generated).unwrap();
    }
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        generated,
        "generated bindings changed; rerun with MESH_BLESS=1 and review the diff"
    );
    assert_eq!(audio::SCHEMA_FINGERPRINT, contract.schema_fingerprint);
}

#[derive(Default)]
struct Provider {
    calls: Vec<String>,
}

impl audio::Provider for Provider {
    fn set_volume(&mut self, device_id: String, percent: f64) -> Result<audio::CallResult, String> {
        self.calls.push(format!("set_volume {device_id} {percent}"));
        Ok(audio::CallResult {
            ok: true,
            error: None,
        })
    }

    fn toggle_mute(&mut self) -> Result<audio::CallResult, String> {
        Err("no sink".into())
    }

    fn r#move(&mut self, r#type: Option<i64>) -> Result<(), String> {
        self.calls.push(format!("move {type:?}"));
        Ok(())
    }

    fn set_app_volume(
        &mut self,
        app: String,
        _percent: f64,
    ) -> Result<audio::types::Device, String> {
        Ok(audio::types::Device {
            id: app,
            channels: 2,
            port: None,
        })
    }
}

#[test]
fn dispatch_decodes_arguments_and_encodes_results() {
    let mut provider = Provider::default();
    assert_eq!(
        audio::dispatch(
            &mut provider,
            "set_volume",
            json!({ "device_id": "sink", "percent": 40.0 })
        ),
        Ok(json!({ "ok": true }))
    );
    assert_eq!(
        audio::dispatch(&mut provider, "move", json!({})),
        Ok(serde_json::Value::Null)
    );
    assert_eq!(
        audio::dispatch(&mut provider, "move", json!({ "type": 3 })),
        Ok(serde_json::Value::Null)
    );
    assert_eq!(
        audio::dispatch(
            &mut provider,
            "set_app_volume",
            json!({ "app": "mpv", "percent": 1 })
        ),
        Ok(json!({ "id": "mpv", "channels": 2 }))
    );
    assert_eq!(
        provider.calls,
        ["set_volume sink 40", "move None", "move Some(3)"]
    );

    assert_eq!(
        audio::dispatch(&mut provider, "toggle_mute", serde_json::Value::Null),
        Err(audio::DispatchError::Failed {
            method: "toggle_mute",
            message: "no sink".into()
        })
    );
    assert!(matches!(
        audio::dispatch(&mut provider, "set_volume", json!({ "percent": "loud" })),
        Err(audio::DispatchError::InvalidArguments {
            method: "set_volume",
            ..
        })
    ));
    assert_eq!(
        audio::dispatch(&mut provider, "nope", json!({})),
        Err(audio::DispatchError::UnknownMethod("nope".into()))
    );
}

#[test]
fn state_and_events_serialize_to_contract_valid_json() {
    let declaration: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(fixture_dir().join("fixtures/audio.contract.json")).unwrap(),
    )
    .unwrap();
    let contract =
        mesh_core_service::parse_compiled_contract("mesh.audio", "1.2", &declaration).unwrap();

    let state = audio::State {
        percent: 40.0,
        art_path: Some("/tmp/a.png".into()),
        devices: vec![audio::types::Device {
            id: "sink".into(),
            channels: 2,
            port: Some(audio::types::Port { name: "out".into() }),
        }],
        ..Default::default()
    };
    let state = serde_json::to_value(&state).unwrap();
    assert_eq!(state["artPath"], "/tmp/a.png");
    assert!(state.get("apps").is_none());
    assert_eq!(contract.state_warnings(&state), Vec::<String>::new());

    let event = audio::Event::VolumeChanged(audio::events::VolumeChanged {
        device_id: "sink".into(),
        level: 0.5,
    });
    assert_eq!(event.name(), "VolumeChanged");
    assert_eq!(
        contract.event_warnings(event.name(), &event.payload()),
        Vec::<String>::new()
    );
    assert_eq!(
        audio::Event::Reset(audio::events::Reset {}).payload(),
        json!({})
    );
}
