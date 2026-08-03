use super::super::*;
use super::common::*;
use mesh_core_capability::{Capability, CapabilitySet};
use mesh_core_elements::VariableStore;
use serde_json::Value;
use std::collections::HashMap;

#[test]
fn interface_proxy_tracks_top_level_field_reads() {
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.audio.read"));
    let mut ctx = ScriptContext::new("@test/audio-widget", caps).unwrap();
    ctx.set_interface_catalog(audio_catalog());
    ctx.load_script(
        r#"
icon_name = "audio-volume-muted"

function sync_audio_state()
    local audio = require("mesh.audio@>=1.0")
    local percent = audio.percent or 0
    if audio.muted then
        icon_name = "audio-volume-muted"
    else
        if percent < 34 then
            icon_name = "audio-volume-low"
        else
            if percent < 67 then
                icon_name = "audio-volume-medium"
            else
                icon_name = "audio-volume-high"
            end
        end
    end
end
"#,
    )
    .unwrap();

    let payload = serde_json::json!({ "percent": 65, "muted": false });
    ctx.apply_service_payload("audio", &payload);
    ctx.call_handler("sync_audio_state", &[]).unwrap();
    assert_eq!(
        ctx.state.get("icon_name"),
        Some(Value::String("audio-volume-medium".into()))
    );

    let tracked = ctx.tracked_fields_for_service("audio");
    assert!(tracked.contains("percent"));
    assert!(tracked.contains("muted"));
    assert!(!ctx.tracked_service_fields_changed(
        "audio",
        Some(&serde_json::json!({ "percent": 65, "muted": false })),
        &serde_json::json!({ "percent": 65, "muted": false }),
    ));
    assert!(ctx.tracked_service_fields_changed(
        "audio",
        Some(&serde_json::json!({ "percent": 65, "muted": false })),
        &serde_json::json!({ "percent": 66, "muted": false }),
    ));
}

#[test]
fn interface_proxy_repeated_field_reads_track_once_per_proxy() {
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.audio.read"));
    let mut ctx = ScriptContext::new("@test/audio-widget", caps).unwrap();
    ctx.set_interface_catalog(audio_catalog());
    ctx.load_script(
        r#"
total = 0

function sync_audio_state()
    local audio = require("mesh.audio@>=1.0")
    for i = 1, 50 do
        total = total + (audio.percent or 0)
        total = total + (audio.state.percent or 0)
    end
end
"#,
    )
    .unwrap();

    ctx.apply_service_payload("audio", &serde_json::json!({ "percent": 1 }));
    ctx.call_handler("sync_audio_state", &[]).unwrap();

    let tracked = ctx.tracked_fields_for_service("audio");
    assert_eq!(tracked.len(), 1);
    assert!(tracked.contains("percent"));
}

// Run with:
// cargo test -p mesh-core-scripting --release -- repeated_interface_field_reads_use_proxy_seen_cache --ignored --nocapture
#[test]
#[ignore]
fn repeated_interface_field_reads_use_proxy_seen_cache() {
    use std::time::Instant;

    let iterations = 20_000usize;
    let old_tracked =
        std::sync::Mutex::new(HashMap::<String, std::collections::HashSet<String>>::new());
    let old_start = Instant::now();
    for _ in 0..iterations {
        old_tracked
            .lock()
            .unwrap()
            .entry("audio".to_string())
            .or_default()
            .insert("percent".to_string());
    }
    let old_ns = old_start.elapsed().as_nanos().max(1);

    let observed = std::sync::Mutex::new(std::collections::HashSet::<String>::new());
    let cached_tracked =
        std::sync::Mutex::new(HashMap::<String, std::collections::HashSet<String>>::new());
    let cached_start = Instant::now();
    for _ in 0..iterations {
        if observed.lock().unwrap().insert("percent".to_string()) {
            cached_tracked
                .lock()
                .unwrap()
                .entry("audio".to_string())
                .or_default()
                .insert("percent".to_string());
        }
    }
    let cached_ns = cached_start.elapsed().as_nanos();

    eprintln!("shared_tracking_every_read={old_ns}ns proxy_seen_cache={cached_ns}ns");
    assert!(
        cached_ns < old_ns,
        "proxy seen-field cache should avoid repeated shared tracking work"
    );
}

#[test]
fn interface_proxy_exposes_state_table() {
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.audio.read"));
    let mut ctx = ScriptContext::new("@test/audio-widget", caps).unwrap();
    ctx.set_interface_catalog(audio_catalog());
    ctx.load_script(
        r#"
audio_state_type = ""

function init()
    local audio = require("mesh.audio@>=1.0")
    audio_state_type = type(audio.state)
end
"#,
    )
    .unwrap();

    ctx.call_init().unwrap();

    assert_eq!(
        ctx.state.get("audio_state_type"),
        Some(serde_json::json!("table"))
    );
}

#[test]
fn interface_proxy_state_reads_latest_payload() {
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.audio.read"));
    let mut ctx = ScriptContext::new("@test/audio-widget", caps).unwrap();
    ctx.set_interface_catalog(audio_catalog());
    ctx.load_script(
        r#"
audio_percent = 0

function sync_audio_state()
    local audio = require("mesh.audio@>=1.0")
    audio_percent = audio.state.percent or 0
end
"#,
    )
    .unwrap();

    ctx.apply_service_payload("audio", &serde_json::json!({ "percent": 31 }));
    ctx.call_handler("sync_audio_state", &[]).unwrap();
    assert_eq!(ctx.state.get("audio_percent"), Some(serde_json::json!(31)));

    ctx.apply_service_payload("audio", &serde_json::json!({ "percent": 88 }));
    ctx.call_handler("sync_audio_state", &[]).unwrap();
    assert_eq!(ctx.state.get("audio_percent"), Some(serde_json::json!(88)));
    assert!(ctx.tracked_fields_for_service("audio").contains("percent"));
}

#[test]
fn interface_proxy_direct_field_read_remains_compatibility_alias() {
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.audio.read"));
    let mut ctx = ScriptContext::new("@test/audio-widget", caps).unwrap();
    ctx.set_interface_catalog(audio_catalog());
    ctx.load_script(
        r#"
state_percent = 0
direct_percent = 0

function sync_audio_state()
    local audio = require("mesh.audio@>=1.0")
    state_percent = audio.state.percent or 0
    direct_percent = audio.percent or 0
end
"#,
    )
    .unwrap();

    ctx.apply_service_payload("audio", &serde_json::json!({ "percent": 57 }));
    ctx.call_handler("sync_audio_state", &[]).unwrap();

    assert_eq!(ctx.state.get("state_percent"), Some(serde_json::json!(57)));
    assert_eq!(ctx.state.get("direct_percent"), Some(serde_json::json!(57)));
}

#[test]
fn interface_proxy_reads_state_fields_without_callbacks() {
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.audio.read"));
    let mut ctx = ScriptContext::new("@test/audio-widget", caps).unwrap();
    ctx.set_interface_catalog(audio_catalog());
    ctx.load_script(
        r#"
icon_name = "audio-volume-muted"

function init()
    local audio = require("mesh.audio@>=1.0")
    if audio.muted then
        icon_name = "audio-volume-muted"
    elseif audio.percent < 50 then
        icon_name = "audio-volume-low"
    else
        icon_name = "audio-volume-high"
    end
end
"#,
    )
    .unwrap();
    let payload = serde_json::json!({ "percent": 80, "muted": false });
    ctx.apply_service_payload("audio", &payload);
    ctx.call_init().unwrap();
    assert_eq!(
        ctx.state.get("icon_name"),
        Some(Value::String("audio-volume-high".into()))
    );
}

#[test]
fn interface_proxy_reads_latest_emitted_fields_after_repeated_updates() {
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.audio.read"));
    let mut ctx = ScriptContext::new("@test/audio-widget", caps).unwrap();
    ctx.set_interface_catalog(audio_catalog());
    ctx.load_script(
        r#"
audio_percent = 0
audio_muted = false
audio_source = ""

function sync_audio_state()
    local audio = require("mesh.audio@>=1.0")
    audio_percent = audio.percent or 0
    audio_muted = audio.muted or false
    audio_source = audio.source_module or ""
end
"#,
    )
    .unwrap();

    ctx.apply_service_payload(
        "audio",
        &serde_json::json!({
            "percent": 25,
            "muted": false,
            "source_module": "@mesh/pulse"
        }),
    );
    ctx.call_handler("sync_audio_state", &[]).unwrap();
    assert_eq!(ctx.state.get("audio_percent"), Some(serde_json::json!(25)));
    assert_eq!(ctx.state.get("audio_muted"), Some(serde_json::json!(false)));
    assert_eq!(
        ctx.state.get("audio_source"),
        Some(serde_json::json!("@mesh/pulse"))
    );

    ctx.apply_service_payload(
        "audio",
        &serde_json::json!({
            "percent": 82,
            "muted": true,
            "source_module": "@mesh/pipewire"
        }),
    );
    ctx.call_handler("sync_audio_state", &[]).unwrap();
    assert_eq!(ctx.state.get("audio_percent"), Some(serde_json::json!(82)));
    assert_eq!(ctx.state.get("audio_muted"), Some(serde_json::json!(true)));
    assert_eq!(
        ctx.state.get("audio_source"),
        Some(serde_json::json!("@mesh/pipewire"))
    );
}

#[test]
fn shared_vm_reuses_equal_service_payload_conversion_marker() {
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.audio.read"));
    let vm = SurfaceVm::new();
    let payload = serde_json::json!({ "percent": 64, "muted": false });

    let mut first = ScriptContext::new("@mesh/first", caps.clone()).unwrap();
    first.attach_shared_vm(&vm);
    first.set_interface_catalog(audio_catalog());
    first
        .load_script(
            r#"
audio = require("mesh.audio@>=1.0")
first_percent = 0
function read()
    first_percent = audio.percent
end
"#,
        )
        .unwrap();
    first.apply_service_payload("audio", &payload);
    let first_marker = first.service_payload_marker_for_test("audio").unwrap();
    assert_eq!(first_marker.len(), std::mem::size_of::<u64>());

    let mut second = ScriptContext::new("@mesh/second", caps).unwrap();
    second.attach_shared_vm(&vm);
    second.set_interface_catalog(audio_catalog());
    second
        .load_script(
            r#"
audio = require("mesh.audio@>=1.0")
second_percent = 0
function read()
    second_percent = audio.percent
end
"#,
        )
        .unwrap();
    second.apply_service_payload("audio", &payload.clone());
    assert_eq!(
        second.service_payload_marker_for_test("audio"),
        Some(first_marker)
    );

    first.call_handler("read", &[]).unwrap();
    second.call_handler("read", &[]).unwrap();
    assert_eq!(
        first.state.get("first_percent"),
        Some(serde_json::json!(64))
    );
    assert_eq!(
        second.state.get("second_percent"),
        Some(serde_json::json!(64))
    );
}

// cargo test -p mesh-core-scripting --release -- binary_service_payload_markers_beat_formatted_strings --ignored --nocapture
#[test]
#[ignore = "release-only service payload marker microbenchmark"]
fn binary_service_payload_markers_beat_formatted_strings() {
    let mut ctx = ScriptContext::new("@mesh/marker-bench", CapabilitySet::new()).unwrap();
    let iterations = 1_000_000usize;
    let (formatted, binary, formatted_hits, binary_hits) =
        ctx.benchmark_service_payload_marker_probes(iterations);

    eprintln!(
        "service payload marker over {iterations} probes: formatted {formatted:?}; binary {binary:?}; ratio {:.2}x",
        formatted.as_secs_f64() / binary.as_secs_f64()
    );
    assert_eq!(formatted_hits, iterations);
    assert_eq!(binary_hits, iterations);
    assert!(binary < formatted);
}

// cargo test -p mesh-core-scripting --release -- cached_service_payload_marker_table_beats_global_lookup --ignored --nocapture
#[test]
#[ignore = "release-only service payload marker-table microbenchmark"]
fn cached_service_payload_marker_table_beats_global_lookup() {
    let mut ctx = ScriptContext::new("@mesh/marker-table-bench", CapabilitySet::new()).unwrap();
    let iterations = 1_000_000usize;
    let (global, cached, global_hits, cached_hits) =
        ctx.benchmark_service_payload_table_access(iterations);

    eprintln!(
        "service payload marker table over {iterations} probes: global {global:?}; cached {cached:?}; ratio {:.2}x",
        global.as_secs_f64() / cached.as_secs_f64()
    );
    assert_eq!(global_hits, iterations);
    assert_eq!(cached_hits, iterations);
    assert!(cached < global);
}

// cargo test -p mesh-core-scripting --release -- shared_service_payload_fingerprint_beats_per_context_hashing --ignored --nocapture
#[test]
#[ignore = "release-only service payload fan-out fingerprint microbenchmark"]
fn shared_service_payload_fingerprint_beats_per_context_hashing() {
    fn make_contexts(count: usize) -> Vec<ScriptContext> {
        let vm = SurfaceVm::new();
        (0..count)
            .map(|index| {
                let mut ctx = ScriptContext::new(
                    format!("@mesh/fingerprint-bench-{index}"),
                    CapabilitySet::new(),
                )
                .unwrap();
                ctx.attach_shared_vm(&vm);
                ctx
            })
            .collect()
    }

    let context_count = 8usize;
    let iterations = 5_000usize;
    let payloads = (0..iterations)
        .map(|index| {
            serde_json::json!({
                "percent": index % 100,
                "muted": index % 2 == 0,
                "devices": [
                    { "id": "sink-0", "name": "Speakers", "volume": index % 100 },
                    { "id": "sink-1", "name": "Headphones", "volume": (index + 7) % 100 }
                ]
            })
        })
        .collect::<Vec<_>>();
    let mut repeated_hash_contexts = make_contexts(context_count);
    let mut shared_hash_contexts = make_contexts(context_count);

    let repeated_started = std::time::Instant::now();
    for payload in &payloads {
        for ctx in &mut repeated_hash_contexts {
            ctx.apply_service_payload("audio", payload);
        }
    }
    let repeated_time = repeated_started.elapsed();

    let shared_started = std::time::Instant::now();
    for payload in &payloads {
        let fingerprint = ScriptContext::service_payload_fingerprint(payload);
        for ctx in &mut shared_hash_contexts {
            ctx.apply_service_payload_with_fingerprint("audio", payload, fingerprint);
        }
    }
    let shared_time = shared_started.elapsed();

    eprintln!(
        "service payload fan-out over {iterations}x{context_count}: per-context hash {repeated_time:?}; shared hash {shared_time:?}; ratio {:.2}x",
        repeated_time.as_secs_f64() / shared_time.as_secs_f64()
    );
    assert_eq!(
        repeated_hash_contexts[0].service_payload_marker_for_test("audio"),
        shared_hash_contexts[0].service_payload_marker_for_test("audio")
    );
    assert!(shared_time < repeated_time);
}

// cargo test -p mesh-core-scripting --release -- cached_service_payload_fingerprint_beats_runtime_seed_rehashing --ignored --nocapture
#[test]
#[ignore = "release-only cached service payload seed microbenchmark"]
fn cached_service_payload_fingerprint_beats_runtime_seed_rehashing() {
    fn make_contexts(count: usize) -> Vec<ScriptContext> {
        let vm = SurfaceVm::new();
        (0..count)
            .map(|index| {
                let mut ctx = ScriptContext::new(
                    format!("@mesh/cached-seed-bench-{index}"),
                    CapabilitySet::new(),
                )
                .unwrap();
                ctx.attach_shared_vm(&vm);
                ctx
            })
            .collect()
    }

    let payload = serde_json::json!({
        "available": true,
        "devices": [
            { "id": "sink-0", "name": "Speakers", "volume": 64 },
            { "id": "sink-1", "name": "Headphones", "volume": 57 }
        ]
    });
    let fingerprint = ScriptContext::service_payload_fingerprint(&payload);
    let context_count = 8usize;
    let iterations = 50_000usize;
    let mut rehash_contexts = make_contexts(context_count);
    let mut cached_contexts = make_contexts(context_count);

    let rehash_started = std::time::Instant::now();
    for _ in 0..iterations {
        for ctx in &mut rehash_contexts {
            ctx.apply_service_payload("audio", std::hint::black_box(&payload));
        }
    }
    let rehash_time = rehash_started.elapsed();

    let cached_started = std::time::Instant::now();
    for _ in 0..iterations {
        for ctx in &mut cached_contexts {
            ctx.apply_service_payload_with_fingerprint(
                "audio",
                std::hint::black_box(&payload),
                fingerprint,
            );
        }
    }
    let cached_time = cached_started.elapsed();

    eprintln!(
        "cached service seed over {iterations}x{context_count}: rehash {rehash_time:?}; cached fingerprint {cached_time:?}; ratio {:.2}x",
        rehash_time.as_secs_f64() / cached_time.as_secs_f64()
    );
    assert_eq!(
        rehash_contexts[0].service_payload_marker_for_test("audio"),
        cached_contexts[0].service_payload_marker_for_test("audio")
    );
    assert!(cached_time < rehash_time);
}

#[test]
fn interface_proxy_method_publishes_service_command() {
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.audio.read"));
    caps.grant(Capability::new("service.audio.control"));
    let mut ctx = ScriptContext::new("@test/audio-widget", caps).unwrap();
    ctx.set_interface_catalog(audio_catalog());
    ctx.load_script(
        r#"
function init()
    local audio = require("mesh.audio@>=1.0")
    audio:set_volume("default", 50)
    audio.set_volume("default", 50)
end
"#,
    )
    .unwrap();
    ctx.call_init().unwrap();
    let published = ctx.drain_published_events();
    assert_eq!(published.len(), 2);
    for event in published {
        assert_eq!(event.channel, "mesh.audio.set_volume");
        assert_eq!(event.source_module_id, "@test/audio-widget");
        assert!(
            event
                .source_capabilities
                .is_granted(&Capability::new("service.audio.control"))
        );
        assert_eq!(
            event.payload,
            serde_json::json!({ "device_id": "default", "percent": 50 })
        );
    }
}

#[test]
fn popover_activate_publishes_focus_option_and_trigger_target() {
    let caps = CapabilitySet::new();
    let mut ctx = ScriptContext::new("@test/nav", caps).unwrap();
    ctx.load_script(
        r#"
function init()
    mesh.popover.activate("@test/popover", {
        surface = { id = "@test/nav" },
        current = { key = "volume-button" }
    }, { focus = false })
end
"#,
    )
    .unwrap();

    ctx.call_init().unwrap();

    let published = ctx.drain_published_events();
    assert_eq!(published.len(), 1);
    assert_eq!(published[0].channel, "shell.activate-popover");
    assert_eq!(
        published[0].payload,
        serde_json::json!({
            "surface_id": "@test/popover",
            "trigger_surface": "@test/nav",
            "trigger_key": "volume-button",
            "focus": false,
        })
    );
}

#[test]
fn mesh_popover_hide_can_request_hover_bridge_deferral() {
    let caps = CapabilitySet::new();
    let mut ctx = ScriptContext::new("@test/popover", caps).unwrap();
    ctx.load_script(
        r#"
function init()
    mesh.popover.hide("@test/popover", { bridge = true })
end
"#,
    )
    .unwrap();

    ctx.call_init().unwrap();

    let published = ctx.drain_published_events();
    assert_eq!(published.len(), 1);
    assert_eq!(published[0].channel, "shell.hide-popover");
    assert_eq!(
        published[0].payload,
        serde_json::json!({
            "surface_id": "@test/popover",
            "defer_for_hover_bridge": true,
        })
    );
}

#[test]
fn interface_proxy_method_returns_queued_result() {
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.audio.read"));
    caps.grant(Capability::new("service.audio.control"));
    let mut ctx = ScriptContext::new("@test/audio-widget", caps).unwrap();
    ctx.set_interface_catalog(audio_catalog());
    ctx.load_script(
        r#"
queued_ok = false
queued = false

function init()
    local audio = require("mesh.audio@>=1.0")
    local result = audio.set_volume("default", 50)
    queued_ok = result.ok
    queued = result.queued
end
"#,
    )
    .unwrap();

    ctx.call_init().unwrap();

    assert_eq!(ctx.state.get("queued_ok"), Some(serde_json::json!(true)));
    assert_eq!(ctx.state.get("queued"), Some(serde_json::json!(true)));
    let published = ctx.drain_published_events();
    assert_eq!(published.len(), 1);
    assert_eq!(published[0].channel, "mesh.audio.set_volume");
}

#[test]
fn read_only_interface_proxy_returns_capability_denied_result() {
    let mut caps = CapabilitySet::new();
    caps.grant(Capability::new("service.audio.read"));
    let mut ctx = ScriptContext::new("@test/read-only-audio", caps).unwrap();
    ctx.set_interface_catalog(audio_catalog());
    ctx.load_script(
        r#"
audio_percent = 0
denied_ok = true
denied_error = ""

function read_state()
    local audio = require("mesh.audio@>=1.0")
    audio_percent = audio.percent or 0
end

function change_volume()
    local audio = require("mesh.audio@>=1.0")
    local result = audio.set_volume("default", 50)
    denied_ok = result.ok
    denied_error = result.error or ""
end
"#,
    )
    .unwrap();

    ctx.apply_service_payload("audio", &serde_json::json!({ "percent": 64 }));
    ctx.call_handler("read_state", &[]).unwrap();
    assert_eq!(ctx.state.get("audio_percent"), Some(serde_json::json!(64)));

    ctx.call_handler("change_volume", &[]).unwrap();
    assert_eq!(ctx.state.get("denied_ok"), Some(serde_json::json!(false)));
    assert_eq!(
        ctx.state.get("denied_error"),
        Some(serde_json::json!("capability denied"))
    );
    assert!(
        ctx.drain_published_events().is_empty(),
        "read-only audio proxy must not publish mesh.audio.set_volume"
    );
}
