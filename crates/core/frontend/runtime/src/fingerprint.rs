use std::hash::{Hash, Hasher};

/// Stable fingerprint for manifest-provided slot props. Slot contributions use
/// JSON values rather than template attributes, but otherwise have the same
/// cache contract as ordinary embedded components.
pub fn slot_props_fingerprint(props: &serde_json::Map<String, serde_json::Value>) -> u64 {
    // `serde_json::Map` is the ordered map representation in this workspace;
    // contribution props originate in a manifest and therefore already have a
    // stable key order. Avoid allocating a sortable scratch vector on every
    // memo probe.
    let mut hasher = std::hash::DefaultHasher::new();
    props.len().hash(&mut hasher);
    for (key, value) in props {
        key.hash(&mut hasher);
        hash_json_value(value, &mut hasher);
    }
    hasher.finish()
}

pub fn hash_json_value(value: &serde_json::Value, hasher: &mut impl Hasher) {
    match value {
        serde_json::Value::Null => 0u8.hash(hasher),
        serde_json::Value::Bool(flag) => {
            1u8.hash(hasher);
            flag.hash(hasher);
        }
        serde_json::Value::Number(number) => {
            2u8.hash(hasher);
            if let Some(int) = number.as_i64() {
                int.hash(hasher);
            } else if let Some(uint) = number.as_u64() {
                uint.hash(hasher);
            } else {
                number.as_f64().unwrap_or(0.0).to_bits().hash(hasher);
            }
        }
        serde_json::Value::String(text) => {
            3u8.hash(hasher);
            text.hash(hasher);
        }
        serde_json::Value::Array(items) => {
            4u8.hash(hasher);
            items.len().hash(hasher);
            for item in items {
                hash_json_value(item, hasher);
            }
        }
        serde_json::Value::Object(entries) => {
            5u8.hash(hasher);
            entries.len().hash(hasher);
            for (key, entry) in entries {
                key.hash(hasher);
                hash_json_value(entry, hasher);
            }
        }
    }
}
