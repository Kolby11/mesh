//! Bounded JSON projections of theme state. Details are revision-checked UTF-8
//! fragments so even a single large token or provenance record is inspectable.
use serde_json::{Value, json};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

pub const INLINE_BYTES: usize = 24 * 1024;
pub const CHUNK_BYTES: usize = 4096;

fn encoded(value: &Value) -> String {
    serde_json::to_string(value).expect("JSON values serialize")
}

/// An immutable serialized detail document. Clones share its UTF-8 storage;
/// retaining a document keeps its content revision independent of later inputs.
#[derive(Clone, Debug)]
pub struct PreparedDetails {
    text: Arc<str>,
    descriptor: Value,
}

impl PreparedDetails {
    /// Serialize a complete JSON document and compute its opaque content revision.
    pub fn new(full: &Value) -> Self {
        let text: Arc<str> = encoded(full).into();
        let descriptor = describe_text(&text);
        Self { text, descriptor }
    }

    /// Describe this complete document without serializing its source again.
    pub fn descriptor(&self) -> Value {
        self.descriptor.clone()
    }

    /// Read a revision-checked fragment with byte offsets on UTF-8 boundaries.
    pub fn read(&self, request: &Value) -> Value {
        read_fragment(&self.text, &self.descriptor, request)
    }
}

/// Describe the complete JSON document with an opaque content revision.
pub fn detail_descriptor(full: &Value) -> Value {
    describe_text(&encoded(full))
}

fn describe_text(text: &str) -> Value {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hash);
    json!({"revision": format!("{:016x}", hash.finish()), "bytes": text.len(), "chunk_bytes": CHUNK_BYTES, "encoding": "json-utf8"})
}

/// Keep small snapshots compatible; large collections move to detail transport.
/// The returned object always fits the inline budget, including its descriptor.
pub fn bounded_snapshot(full: Value) -> Value {
    let details = detail_descriptor(&full);
    project_snapshot(full, details)
}

/// Project the inline snapshot and retain its complete immutable detail document.
pub fn prepare_snapshot(full: Value) -> (Value, PreparedDetails) {
    let prepared = PreparedDetails::new(&full);
    (project_snapshot(full, prepared.descriptor()), prepared)
}

fn project_snapshot(mut full: Value, details: Value) -> Value {
    for keys in [
        &["tokens", "provenance"][..],
        &["themes", "available", "modes", "system_resources"][..],
    ] {
        if encoded(&full).len() + encoded(&details).len() + 16 <= INLINE_BYTES {
            break;
        }
        for key in keys {
            full.as_object_mut()
                .expect("theme state is an object")
                .remove(*key);
        }
    }
    full["details"] = details.clone();
    if encoded(&full).len() > INLINE_BYTES {
        return json!({"details": details});
    }
    full
}

/// Read a fragment of a complete JSON snapshot. Offsets count UTF-8 bytes;
/// responses never split a code point. A changed source rejects the old cursor.
pub fn read_details(full: &Value, request: &Value) -> Value {
    PreparedDetails::new(full).read(request)
}

fn read_fragment(text: &str, descriptor: &Value, request: &Value) -> Value {
    if request["revision"] != descriptor["revision"] {
        return json!({"ok": false, "error": "stale_revision", "details": descriptor});
    }
    let offset = match request.get("offset") {
        None | Some(Value::Null) => 0,
        Some(value) => match value.as_u64() {
            Some(offset) => offset,
            None => return json!({"ok": false, "error": "invalid_offset"}),
        },
    };
    let Ok(offset) = usize::try_from(offset) else {
        return json!({"ok": false, "error": "invalid_offset"});
    };
    if offset > text.len() || !text.is_char_boundary(offset) {
        return json!({"ok": false, "error": "invalid_offset"});
    }
    let limit = request["limit"]
        .as_u64()
        .unwrap_or(CHUNK_BYTES as u64)
        .clamp(4, CHUNK_BYTES as u64) as usize;
    let mut end = offset.saturating_add(limit).min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    json!({"ok": true, "status": "applied", "revision": descriptor["revision"], "data": &text[offset..end], "next_offset": (end < text.len()).then_some(end), "total_bytes": text.len()})
}

/// Large event values are read through the last change's detail document,
/// including removed tokens. Oversized identities also move to that document.
pub fn bounded_event(mut event: Value, details: &Value) -> Value {
    if encoded(&event).len() > INLINE_BYTES {
        for key in [
            "tokens",
            "provenance",
            "changed_tokens",
            "value",
            "modes",
            "mode_policy",
        ] {
            event
                .as_object_mut()
                .expect("theme event is an object")
                .remove(key);
        }
        event["details"] = details.clone();
        event["details_required"] = json!(true);
    }
    if encoded(&event).len() > INLINE_BYTES {
        return json!({"details":details, "details_required":true});
    }
    event
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oversized_token_and_provenance_round_trip_with_escaped_unicode() {
        let full = json!({"theme_id":"test", "tokens":{"large": "\"\\\n🦀".repeat(30000)}, "provenance":{"large": "x".repeat(90000)}});
        let summary = bounded_snapshot(full.clone());
        assert!(encoded(&summary).len() < INLINE_BYTES);
        assert!(summary.get("tokens").is_none());
        let mut offset = 0;
        let mut result = String::new();
        loop {
            let page = read_details(
                &full,
                &json!({"revision":summary["details"]["revision"], "offset":offset,"limit":u64::MAX}),
            );
            assert_eq!(page["ok"], true);
            assert!(encoded(&page).len() < INLINE_BYTES + 1024);
            result.push_str(page["data"].as_str().unwrap());
            let Some(next) = page["next_offset"].as_u64() else {
                break;
            };
            assert!(next > offset);
            offset = next;
        }
        assert_eq!(serde_json::from_str::<Value>(&result).unwrap(), full);
        assert_eq!(
            read_details(
                &json!({"tokens":{}}),
                &json!({"revision":summary["details"]["revision"]})
            )["error"],
            "stale_revision"
        );
        let text = encoded(&full);
        let middle = text.find('🦀').unwrap() + 1;
        assert_eq!(
            read_details(
                &full,
                &json!({"revision":summary["details"]["revision"],"offset":middle})
            )["error"],
            "invalid_offset"
        );
    }

    #[test]
    fn oversized_identities_and_catalogs_also_fit_the_inline_budget() {
        let full = json!({"theme_id":"x".repeat(INLINE_BYTES * 2), "themes":["y".repeat(INLINE_BYTES * 2)]});
        let summary = bounded_snapshot(full.clone());
        assert!(encoded(&summary).len() <= INLINE_BYTES);
        let event = bounded_event(full.clone(), &detail_descriptor(&full));
        assert!(encoded(&event).len() <= INLINE_BYTES);
        assert_eq!(event["details_required"], true);
        assert_eq!(
            read_details(
                &full,
                &json!({"revision":detail_descriptor(&full)["revision"],"offset":-1})
            )["error"],
            "invalid_offset"
        );
        let empty = read_details(
            &full,
            &json!({"revision":detail_descriptor(&full)["revision"],"offset":encoded(&full).len()}),
        );
        assert_eq!(empty["data"], "");
        assert!(empty["next_offset"].is_null());
        assert_eq!(
            read_details(
                &full,
                &json!({"revision":detail_descriptor(&full)["revision"],"offset":u64::MAX})
            )["error"],
            "invalid_offset"
        );
    }

    #[test]
    fn prepared_details_preserve_revision_and_utf8_after_source_changes() {
        let mut full = json!({"tokens":{"text":"é🦀\\\"".repeat(1000)}});
        let original = full.clone();
        let (summary, prepared) = prepare_snapshot(full.clone());
        let shared = prepared.clone();
        assert!(Arc::ptr_eq(&prepared.text, &shared.text));
        assert_eq!(summary["details"], prepared.descriptor());
        full["tokens"]["text"] = json!("changed");
        let replacement = PreparedDetails::new(&full);
        let mut offset = 0;
        let mut text = String::new();
        loop {
            let request =
                json!({"revision":summary["details"]["revision"], "offset":offset, "limit":5});
            let page = prepared.read(&request);
            assert_eq!(page, read_details(&original, &request));
            assert_eq!(replacement.read(&request)["error"], "stale_revision");
            text.push_str(page["data"].as_str().unwrap());
            let Some(next) = page["next_offset"].as_u64() else {
                break;
            };
            assert!(next > offset);
            offset = next;
        }
        assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), original);
        let middle = prepared.text.find('🦀').unwrap() + 1;
        for offset in [json!(middle), json!(-1), json!(u64::MAX)] {
            assert_eq!(
                shared.read(&json!({"revision":summary["details"]["revision"], "offset":offset}))["error"],
                "invalid_offset"
            );
        }
    }

    #[test]
    #[ignore = "release-only paired detail transport measurement"]
    fn prepared_theme_details_workload() {
        use std::hint::black_box;
        use std::time::Instant;

        for count in [16, 512, 4096] {
            let tokens: serde_json::Map<String, Value> = (0..count)
                .map(|index| (format!("color.token{index}"), json!("é🦀\\\"".repeat(12))))
                .collect();
            let full = json!({"theme_id":"measurement", "tokens":tokens});
            let prepared = PreparedDetails::new(&full);
            let retained_source = full.clone();
            let mut requests = Vec::new();
            let mut offset = 0;
            loop {
                let request = json!({"revision":prepared.descriptor["revision"], "offset":offset});
                let page = prepared.read(&request);
                requests.push(request);
                let Some(next) = page["next_offset"].as_u64() else {
                    break;
                };
                offset = next;
            }
            let mut baseline = Vec::new();
            let mut reused = Vec::new();
            for sample in 0..7 {
                let mut run = |reuse| {
                    let start = Instant::now();
                    for request in &requests {
                        let page = if reuse {
                            assert_eq!(black_box(&full), black_box(&retained_source));
                            prepared.read(black_box(request))
                        } else {
                            let descriptor = detail_descriptor(black_box(&full));
                            let text = encoded(black_box(&full));
                            read_fragment(&text, &descriptor, request)
                        };
                        black_box(page);
                    }
                    if reuse {
                        reused.push(start.elapsed());
                    } else {
                        baseline.push(start.elapsed());
                    }
                };
                run(sample % 2 == 0);
                run(sample % 2 != 0);
            }
            baseline.sort();
            reused.sort();
            println!(
                "tokens={count} bytes={} pages={} samples=7 baseline_min_us={} baseline_p50_us={} baseline_max_us={} prepared_min_us={} prepared_p50_us={} prepared_max_us={}",
                prepared.text.len(),
                requests.len(),
                baseline[0].as_micros(),
                baseline[3].as_micros(),
                baseline[6].as_micros(),
                reused[0].as_micros(),
                reused[3].as_micros(),
                reused[6].as_micros()
            );
            if count == 4096 {
                let speedup = baseline[3].as_secs_f64() / reused[3].as_secs_f64();
                println!("MESH_PERF metric=prepared_theme_details_speedup value={speedup:.6}");
                assert!(
                    speedup >= 2.0,
                    "prepared detail reads must avoid document serialization"
                );
            }
        }
    }

    #[test]
    fn large_event_keeps_identity_revision_and_change_count() {
        let event = bounded_event(
            json!({"name":"color.a", "revision":"123", "changed_token_count":10000, "value":"x".repeat(100000)}),
            &json!({"revision":"456"}),
        );
        assert!(encoded(&event).len() < INLINE_BYTES);
        assert_eq!(event["name"], "color.a");
        assert_eq!(event["changed_token_count"], 10000);
        assert_eq!(event["details_required"], true);
    }
}
