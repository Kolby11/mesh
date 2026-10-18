use crate::policy::{ResourceBudget, ResourceKind, ResourceLimit};
use serde_json::Value;
use std::sync::LazyLock;

/// An immutable service snapshot checked against the session and transport
/// JSON limits. Construction also reserves the callback's output budget.
#[derive(Debug, Clone, PartialEq)]
pub struct BackendState {
    payload: Value,
    fingerprint: u64,
}

impl BackendState {
    pub(super) fn new(payload: Value, budget: &ResourceBudget) -> Result<Self, ResourceLimit> {
        let config = budget.config();
        let max_bytes =
            (config.json_max_bytes as usize).min(mesh_core_runtime::DEFAULT_JSON_MAX_BYTES);
        let max_depth =
            (config.json_max_depth as usize).min(mesh_core_runtime::DEFAULT_JSON_MAX_DEPTH);
        let bytes =
            mesh_core_runtime::validate_json(&payload, max_bytes, max_depth, "service state")
                .map_err(|_| ResourceLimit {
                    kind: ResourceKind::Output,
                    limit: max_bytes as u64,
                })?;
        budget.reserve_output(bytes)?;
        static HASHER: LazyLock<ahash::RandomState> = LazyLock::new(ahash::RandomState::new);
        let fingerprint = HASHER.hash_one(&payload);
        Ok(Self {
            payload,
            fingerprint,
        })
    }

    /// Content fingerprint for unchanged-state suppression within this process.
    pub fn fingerprint(&self) -> u64 {
        self.fingerprint
    }

    pub fn into_payload(self) -> Value {
        self.payload
    }
}

impl std::ops::Deref for BackendState {
    type Target = Value;

    fn deref(&self) -> &Value {
        &self.payload
    }
}

impl PartialEq<Value> for BackendState {
    fn eq(&self, other: &Value) -> bool {
        self.payload == *other
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core_runtime::SandboxConfig;
    use std::time::Instant;

    #[test]
    fn state_limits_cover_session_transport_and_callback_budgets() {
        let mut config = SandboxConfig::default();
        config.json_max_bytes = 16;
        let budget = ResourceBudget::new(config);
        assert!(
            BackendState::new(serde_json::json!({ "value": "x".repeat(20) }), &budget).is_err()
        );

        let mut config = SandboxConfig::default();
        config.json_max_bytes *= 4;
        config.json_max_depth *= 4;
        let budget = ResourceBudget::new(config);
        assert!(
            BackendState::new(
                Value::String("x".repeat(mesh_core_runtime::DEFAULT_JSON_MAX_BYTES)),
                &budget
            )
            .is_err()
        );
        let mut nested = Value::Null;
        for _ in 0..=mesh_core_runtime::DEFAULT_JSON_MAX_DEPTH {
            nested = serde_json::json!([nested]);
        }
        assert!(BackendState::new(nested, &budget).is_err());

        let mut config = SandboxConfig::default();
        config.output_budget = 12;
        let budget = ResourceBudget::new(config);
        let _guard = budget.begin_callback();
        BackendState::new(serde_json::json!({ "a": 1 }), &budget).unwrap();
        assert!(BackendState::new(serde_json::json!({ "b": 2 }), &budget).is_err());
    }

    #[test]
    fn fingerprints_follow_nested_content_and_ignore_object_insertion_order() {
        let budget = ResourceBudget::new(SandboxConfig::default());
        let a = BackendState::new(
            serde_json::json!({ "a": [1, { "x": true }], "b": "c" }),
            &budget,
        )
        .unwrap();
        let b = BackendState::new(
            serde_json::json!({ "b": "c", "a": [1, { "x": true }] }),
            &budget,
        )
        .unwrap();
        let c = BackendState::new(
            serde_json::json!({ "a": [1, { "x": false }], "b": "c" }),
            &budget,
        )
        .unwrap();
        assert_eq!(a.fingerprint(), b.fingerprint());
        assert_ne!(a.fingerprint(), c.fingerprint());
        assert_eq!(a.into_payload(), b.into_payload());
    }

    fn baseline(payloads: &[Value], iterations: usize) -> std::time::Duration {
        let budget = ResourceBudget::new(SandboxConfig::default());
        let mut last_payload = None;
        let started = Instant::now();
        for index in 0..iterations {
            let _guard = budget.begin_callback();
            let payload = std::hint::black_box(&payloads[index % payloads.len()]).clone();
            let bytes = budget
                .validate_json(std::hint::black_box(&payload), "service payload")
                .unwrap();
            budget.reserve_output(bytes).unwrap();
            budget
                .validate_json(std::hint::black_box(&payload), "service state snapshot")
                .unwrap();
            if last_payload.as_ref() != Some(&payload) {
                mesh_core_runtime::validate_json(
                    std::hint::black_box(&payload),
                    mesh_core_runtime::DEFAULT_JSON_MAX_BYTES,
                    mesh_core_runtime::DEFAULT_JSON_MAX_DEPTH,
                    "service state payload",
                )
                .unwrap();
                last_payload = Some(payload.clone());
                std::hint::black_box(payload);
            }
        }
        std::hint::black_box(last_payload);
        started.elapsed()
    }

    fn validated(payloads: &[Value], iterations: usize) -> std::time::Duration {
        let budget = ResourceBudget::new(SandboxConfig::default());
        let mut last_fingerprint = None;
        let started = Instant::now();
        for index in 0..iterations {
            let _guard = budget.begin_callback();
            let payload = BackendState::new(
                std::hint::black_box(&payloads[index % payloads.len()]).clone(),
                &budget,
            )
            .unwrap();
            if last_fingerprint != Some(payload.fingerprint()) {
                last_fingerprint = Some(payload.fingerprint());
                std::hint::black_box(payload.into_payload());
            }
        }
        std::hint::black_box(last_fingerprint);
        started.elapsed()
    }

    #[test]
    #[ignore = "release-only validated state publication boundary benchmark"]
    fn validated_state_publication_profile() {
        assert!(!cfg!(debug_assertions), "run with --release");
        let rows: Vec<_> = (0..128).map(|index| serde_json::json!({
            "id": index,
            "name": format!("device-{index}"),
            "properties": { "available": true, "level": index, "tags": ["input", "output"] },
        })).collect();
        let first = serde_json::json!({ "devices": rows, "generation": 1 });
        let mut second = first.clone();
        second["generation"] = 2.into();
        for payloads in [vec![first.clone()], vec![first.clone(), second]] {
            let _ = baseline(&payloads, 100);
            let _ = validated(&payloads, 100);
            for run in 0..3 {
                let (before, after) = if run % 2 == 0 {
                    (baseline(&payloads, 2000), validated(&payloads, 2000))
                } else {
                    let after = validated(&payloads, 2000);
                    (baseline(&payloads, 2000), after)
                };
                eprintln!(
                    "service state: rows=128 bytes={} changed={} iterations=2000 run={run} before={before:?} after={after:?} ratio={:.2}",
                    mesh_core_runtime::json_encoded_len(&first),
                    payloads.len() > 1,
                    before.as_secs_f64() / after.as_secs_f64()
                );
                assert!(
                    after.as_secs_f64() * 1.05 < before.as_secs_f64(),
                    "validated_state_publication_speedup must exceed 1.05x"
                );
            }
        }
    }
}
