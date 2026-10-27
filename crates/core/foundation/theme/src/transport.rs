//! Bounded JSON projections of theme state. Details are revision-checked UTF-8
//! fragments so even a single large token or provenance record is inspectable.
use serde_json::{Value, json};
use std::hash::{Hash, Hasher};

pub const INLINE_BYTES: usize = 24 * 1024;
pub const CHUNK_BYTES: usize = 4096;

fn encoded(value: &Value) -> String {
    serde_json::to_string(value).expect("JSON values serialize")
}

/// Describe the complete JSON document with an opaque content revision.
pub fn detail_descriptor(full: &Value) -> Value {
    let text = encoded(full);
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hash);
    json!({"revision": format!("{:016x}", hash.finish()), "bytes": text.len(), "chunk_bytes": CHUNK_BYTES, "encoding": "json-utf8"})
}

/// Keep small snapshots compatible; large collections move to detail transport.
/// The returned object always fits the inline budget, including its descriptor.
pub fn bounded_snapshot(mut full: Value) -> Value {
    let details = detail_descriptor(&full);
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
    let descriptor = detail_descriptor(full);
    if request["revision"] != descriptor["revision"] {
        return json!({"ok": false, "error": "stale_revision", "details": descriptor});
    }
    let text = encoded(full);
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
