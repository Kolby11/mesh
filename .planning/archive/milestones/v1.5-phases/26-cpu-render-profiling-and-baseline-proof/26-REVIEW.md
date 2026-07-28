---
phase: 26-cpu-render-profiling-and-baseline-proof
reviewed: 2026-05-11T04:07:10Z
depth: standard
files_reviewed: 8
files_reviewed_list:
  - crates/core/frontend/render/src/surface/glyph.rs
  - crates/core/frontend/render/src/surface/mod.rs
  - crates/core/shell/src/shell/tests.rs
  - crates/core/shell/src/shell/component/tests.rs
  - .planning/phases/26-cpu-render-profiling-and-baseline-proof/26-01-BASELINE.md
  - .planning/phases/26-cpu-render-profiling-and-baseline-proof/26-01-SUMMARY.md
  - .planning/phases/26-cpu-render-profiling-and-baseline-proof/26-01-PLAN.md
  - .planning/REQUIREMENTS.md
findings:
  critical: 1
  warning: 0
  info: 0
  total: 1
status: issues_found
---

# Phase 26: Code Review Report

**Reviewed:** 2026-05-11T04:07:10Z
**Depth:** standard
**Files Reviewed:** 8
**Status:** issues_found

## Summary

Re-reviewed Phase 26 after the shipped-surface proof follow-up. The narrow "synthetic-only evidence" objection is no longer accurate: `phase26_real_surface_baseline_emits_canonical_proof_measurements` now emits real timings from shipped frontend components, and the retained-attribution fixes in `glyph.rs` and `surface/mod.rs` still hold.

Phase 26 is still blocked, though, because the new real-surface capture does not actually exercise the canonical `surface_open_close` and `pointer_update` scenarios it claims in the baseline artifact. That means the committed proof still overstates PERF-02 completion even though it now contains real measurements.

Verification run:
- `nix develop -c cargo test -p mesh-core-shell phase26_real_surface_baseline_emits_canonical_proof_measurements -- --nocapture`
- `nix develop -c cargo test -p mesh-core-shell phase26_baseline_proof_records_canonical_scenario_values_and_retained_hotspots -- --nocapture`

## Critical Issues

### CR-01: Baseline artifact still labels non-canonical component paints as canonical benchmark proof

**Classification:** BLOCKER
**File:** `.planning/phases/26-cpu-render-profiling-and-baseline-proof/26-01-BASELINE.md:7-37`, `.planning/phases/26-cpu-render-profiling-and-baseline-proof/26-01-SUMMARY.md:47-48`, `.planning/phases/26-cpu-render-profiling-and-baseline-proof/26-01-SUMMARY.md:61-62`, `crates/core/shell/src/shell/component/tests.rs:269-325`
**Issue:** The new artifact claims real measurements for all five canonical scenarios on their shipped targets, but the proof test does not execute two of those scenarios as defined by the benchmark contract. `surface_open_close` is documented as "Open/close the shipped audio popover" in [26-01-BASELINE.md](/home/kolby/projects/mesh/.planning/phases/26-cpu-render-profiling-and-baseline-proof/26-01-BASELINE.md:10), yet the test only injects a service update into an already-mounted `@mesh/audio-popover` component and paints it once; it never opens or closes the surface. `pointer_update` is documented as the `@mesh/navigation-bar audio controls` scenario in [26-01-BASELINE.md](/home/kolby/projects/mesh/.planning/phases/26-cpu-render-profiling-and-baseline-proof/26-01-BASELINE.md:11), and the benchmark contract still targets that navigation-bar path in [runtime/debug.rs](/home/kolby/projects/mesh/crates/core/shell/src/shell/runtime/debug.rs:288), but the real proof reuses the audio-popover slider drag instead of the shipped navigation-bar control path. These timings are real, but they are not proof for the canonical scenarios the phase claims to have completed, so PERF-02 remains open.
**Fix:**
```rust
let mut shell = Shell::new();
shell.apply_request(CoreRequest::ToggleDebugProfiling)?;

// Drive the actual canonical scenario through the shipped surface path.
shell.apply_request(CoreRequest::RunDebugBenchmark {
    scenario_id: "surface_open_close".into(),
})?;
drain_requests_and_render_until_idle(&mut shell)?;

shell.apply_request(CoreRequest::RunDebugBenchmark {
    scenario_id: "pointer_update".into(),
})?;
exercise_navigation_bar_audio_controls(&mut shell)?;
drain_requests_and_render_until_idle(&mut shell)?;

let snapshot = shell.build_debug_snapshot();
assert_eq!(scenario_by_id(&snapshot, "surface_open_close").status, BenchmarkScenarioStatus::Complete);
assert_eq!(scenario_by_id(&snapshot, "pointer_update").status, BenchmarkScenarioStatus::Complete);
```

If Phase 26 cannot yet drive those canonical flows end-to-end, the docs and summary need to stop claiming PERF-02 completion and explicitly leave the phase open.

## Residual Risks / Testing Gaps

- The deterministic `Shell::build_debug_snapshot()` proof in `tests.rs` is acceptable as a contract lock, but it still does not substitute for real canonical scenario execution.
- The real proof output is currently copied into the baseline artifact by hand, so future edits can drift unless the canonical capture path is made mechanical.

---

_Reviewed: 2026-05-11T04:07:10Z_
_Reviewer: the agent (gsd-code-reviewer)_
_Depth: standard_
