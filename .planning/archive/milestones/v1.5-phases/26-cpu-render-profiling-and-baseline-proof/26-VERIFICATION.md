---
phase: 26-cpu-render-profiling-and-baseline-proof
verified: 2026-05-11T04:06:59Z
status: passed
score: 4/4 must-haves verified
overrides_applied: 0
re_verification:
  previous_status: gaps_found
  previous_score: 3/4
  gaps_closed:
    - "Phase 26 records reusable benchmark evidence for the five canonical shipped scenarios, including the pre-change baseline and post-instrumentation profiling view."
  gaps_remaining: []
  regressions: []
---

# Phase 26: CPU Render Profiling and Baseline Proof Verification Report

**Phase Goal:** Attribute the remaining CPU rendering cost on shipped surfaces and canonical benchmark scenarios before implementation phases begin.
**Verified:** 2026-05-11T04:06:59Z
**Status:** passed
**Re-verification:** Yes - after prior gap report

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
| --- | --- | --- | --- |
| 1 | Developers can inspect retained CPU render cost through the existing profiling/debug path, including render-object sync, retained display-list update, paint traversal, text shaping, and icon/image raster work. | ✓ VERIFIED | `ProfilingStage` still defines the retained substages in `crates/core/foundation/debug/src/lib.rs:289-325`. The shipped paint path records them in `crates/core/shell/src/shell/component/shell_component.rs:295-478`, and `mesh.debug` still publishes the profiling payload in `crates/core/shell/src/shell/runtime/debug.rs:85-150`. Regression checks `cargo test -p mesh-core-shell profiling`, `cargo test -p mesh-core-render`, and `retained_paint_path_records_phase26_cpu_attribution_stages` passed. |
| 2 | The canonical benchmark suite still exposes exactly five stable scenarios with unchanged IDs and shipped-surface targets. | ✓ VERIFIED | `benchmark_snapshot()` still hardcodes `hover`, `surface_open_close`, `pointer_update`, `keyboard_traversal`, and `backend_update` in `crates/core/shell/src/shell/runtime/debug.rs:153-183`, with stable targets verified in `crates/core/shell/src/shell/tests.rs:559-674`. `benchmark_snapshot_exposes_five_stable_scenarios` passed. |
| 3 | Profiling and benchmark payloads stay on the existing `mesh.debug` path and remain inert when profiling is disabled. | ✓ VERIFIED | `build_debug_snapshot()` still serializes both `benchmarks` and optional `profiling` through the existing debug payload in `crates/core/shell/src/shell/runtime/debug.rs:4-150`, while `crates/core/shell/src/shell/tests.rs:549-619` keeps the profiling-disabled path inert. `cargo test -p mesh-core-shell profiling` passed. |
| 4 | Phase 26 records reusable benchmark evidence for the five canonical shipped scenarios, including the pre-change baseline and post-instrumentation profiling view. | ✓ VERIFIED | `.planning/phases/26-cpu-render-profiling-and-baseline-proof/26-01-BASELINE.md:15-100` now cites two proof sources: a real shipped-surface capture from `crates/core/shell/src/shell/component/tests.rs:221-447` and a deterministic benchmark-row contract proof from `crates/core/shell/src/shell/tests.rs:2155-2371`. Re-running `phase26_real_surface_baseline_emits_canonical_proof_measurements -- --nocapture` emitted five non-zero `PHASE26_BASELINE` lines for `hover`, `surface_open_close`, `pointer_update`, `keyboard_traversal`, and `backend_update`, closing the previous “seeded-only” evidence gap. |

**Score:** 4/4 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
| --- | --- | --- | --- |
| `crates/core/foundation/debug/src/lib.rs` | Stable scenario/stage vocabulary for retained CPU attribution | ✓ VERIFIED | `ProfilingStage` includes `render_object_sync`, `retained_display_list_update`, `paint_traversal`, `text_shaping`, and `icon_image_raster` with stable labels in `:289-325`. |
| `crates/core/frontend/render/src/surface/glyph.rs` | Icon/image raster timing records only real raster work | ✓ VERIFIED | Cache misses wrap `rasterize(...)` with `profiling::record_icon_image_raster(...)`; cache hits bypass timing in `:178-227`. |
| `crates/core/frontend/render/src/surface/mod.rs` | Display-list paint reports traversal/text/raster metrics back to the shell | ✓ VERIFIED | `paint_display_list_for_module_with_profiling_metrics(...)` resets counters, measures traversal, and returns `PaintProfilingMetrics` in `:153-187`; the tooltip regression in `:194-232` guards stage contamination. |
| `crates/core/shell/src/shell/component/shell_component.rs` | The shipped retained paint path records the Phase 26 CPU substages | ✓ VERIFIED | The real component paint path records render-object sync, retained display-list update, paint traversal, text shaping, icon/image raster, and paint in `:295-478`. |
| `crates/core/shell/src/shell/runtime/debug.rs` | Existing `mesh.debug` payload exposes benchmarks and profiling without a second system | ✓ VERIFIED | `build_debug_snapshot()` and `debug_service_payload()` still publish `benchmarks` plus optional `profiling` through the current debug contract in `:4-150`. |
| `crates/core/shell/src/shell/component/tests.rs` | Real shipped-surface baseline capture for the five canonical scenarios | ✓ VERIFIED | `phase26_real_surface_baseline_emits_canonical_proof_measurements` drives hover, popover open, pointer update, keyboard traversal, and backend update through real shipped components and prints measured timings in `:221-447`. |
| `crates/core/shell/src/shell/tests.rs` | Deterministic benchmark-row and retained-hotspot contract proof | ✓ VERIFIED | `phase26_baseline_proof_records_canonical_scenario_values_and_retained_hotspots` still seeds fixed values and verifies snapshot formatting/order in `:2155-2371`. This is contract proof, not live measurement, and is now supplemental rather than sole evidence. |
| `.planning/phases/26-cpu-render-profiling-and-baseline-proof/26-01-BASELINE.md` | Reusable baseline artifact for later v1.5 comparisons | ✓ VERIFIED | The artifact now records the five canonical scenarios, a real-surface measured table, pre-change coarse rows, post-instrumentation hotspot rankings, and reuse guidance in `:5-100`. |

### Key Link Verification

| From | To | Via | Status | Details |
| --- | --- | --- | --- | --- |
| `crates/core/frontend/render/src/surface/mod.rs` | `crates/core/shell/src/shell/component/shell_component.rs` | `paint_display_list_for_module_with_profiling_metrics(...)` return value | ✓ VERIFIED | `shell_component.rs:439-478` consumes traversal, shaping, and raster metrics and records them as profiling stages. |
| `crates/core/shell/src/shell/component/shell_component.rs` | `crates/core/shell/src/shell/runtime/debug.rs` | runtime profiling state -> `build_debug_snapshot()` | ✓ VERIFIED | The component records the new stages; `build_debug_snapshot()` publishes them on `mesh.debug` in `crates/core/shell/src/shell/runtime/debug.rs:85-183`. |
| `crates/core/shell/src/shell/component/tests.rs` | `.planning/phases/26-cpu-render-profiling-and-baseline-proof/26-01-BASELINE.md` | cited real-surface proof command and scenario coverage | ✓ VERIFIED | `26-01-BASELINE.md:19-37` cites the real-surface capture, and the test at `component/tests.rs:221-447` executes all five canonical scenario classes on shipped proof surfaces and prints measured timings. |
| `crates/core/shell/src/shell/tests.rs` | `.planning/phases/26-cpu-render-profiling-and-baseline-proof/26-01-BASELINE.md` | cited deterministic benchmark-row/hotspot proof | ✓ VERIFIED | `26-01-BASELINE.md:25-75` cites the seeded snapshot proof, and `tests.rs:2155-2371` verifies the canonical row IDs, coarse metrics, and retained-hotspot ordering that later phases compare against. |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
| --- | --- | --- | --- | --- |
| `crates/core/shell/src/shell/component/shell_component.rs` | `paint_metrics` / `snapshot.text` | `mesh_core_render::paint_display_list_for_module_with_profiling_metrics(...)` and `text_cache_snapshot(...)` | Yes | ✓ FLOWING |
| `crates/core/shell/src/shell/runtime/debug.rs` | `snapshot.profiling` / `snapshot.benchmarks` | `self.profiling.snapshot(...)` plus `benchmark_snapshot(...)` | Yes | ✓ FLOWING |
| `crates/core/shell/src/shell/component/tests.rs` | `PHASE26_BASELINE ...` measurements | Real shipped-component paints and interaction/service-event paths for all five scenarios | Yes | ✓ FLOWING |
| `.planning/phases/26-cpu-render-profiling-and-baseline-proof/26-01-BASELINE.md` | Measured shipped-surface baseline table | Real-surface proof command output recorded alongside deterministic contract rows | Yes | ✓ FLOWING |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
| --- | --- | --- | --- |
| Real shipped-surface baseline capture runs and emits canonical proof measurements | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell phase26_real_surface_baseline_emits_canonical_proof_measurements -- --nocapture` | Passed; emitted five `PHASE26_BASELINE` lines with non-zero timings. Current rerun: hover `paint=3248us`, open/close `paint=33360us`, pointer `paint=1993us`, keyboard `paint=3078us`, backend `paint=31556us`. | ✓ PASS |
| Deterministic canonical-row/hotspot contract stays green | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell phase26_baseline_proof_records_canonical_scenario_values_and_retained_hotspots` | 1 test passed | ✓ PASS |
| Profiling payload and gating regressions hold | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling` | 29 tests passed | ✓ PASS |
| Render crate remains green | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render` | 35 tests passed | ✓ PASS |
| Workspace formatting check | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo fmt --check` | Failed only in unrelated files `crates/core/presentation/src/wayland_surface/backend.rs` and `crates/core/runtime/scripting/src/context/tests.rs` | ✗ FAIL (unrelated workspace drift) |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
| --- | --- | --- | --- | --- |
| `PERF-01` | `26-01-PLAN.md` | Developers can inspect CPU render cost for tree build, style restyle, layout, render-object sync, retained display-list rebuild, paint traversal, text shaping, and icon/image raster work on each canonical benchmark scenario. | ✓ SATISFIED | The retained CPU stages are defined in `crates/core/foundation/debug/src/lib.rs:289-325`, produced by the shipped paint path in `crates/core/shell/src/shell/component/shell_component.rs:295-478`, surfaced through `mesh.debug` in `crates/core/shell/src/shell/runtime/debug.rs:85-183`, and exercised on real shipped surfaces in `crates/core/shell/src/shell/component/tests.rs:221-447`. |
| `PERF-02` | `26-01-PLAN.md` | Every v1.5 optimization phase records before/after benchmark evidence on shipped surfaces using the existing canonical benchmark scenarios. | ✓ SATISFIED | `.planning/phases/26-cpu-render-profiling-and-baseline-proof/26-01-BASELINE.md:15-100` now records the reusable Phase 26 baseline, citing both the real shipped-surface capture in `component/tests.rs:221-447` and the deterministic canonical-row/hotspot contract proof in `tests.rs:2155-2371`. The real-surface proof command passed in this session and emitted live measurements for all five canonical scenarios. |

Orphaned requirements: none. `REQUIREMENTS.md` maps only `PERF-01` and `PERF-02` to Phase 26, and both are now satisfied.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
| --- | --- | --- | --- | --- |
| `crates/core/shell/src/shell/tests.rs` | 2155 | Seeded benchmark-row proof | ℹ Info | This test still seeds fixed profiling values and should not be mistaken for live measurement, but it is now explicitly supplemental to the real-surface proof rather than the only evidence source. |

### Human Verification Required

None.

### Gaps Summary

The previous blocker is closed. Phase 26 now has executable proof on real shipped surfaces for all five canonical scenarios, recorded in `phase26_real_surface_baseline_emits_canonical_proof_measurements`, and the baseline artifact cites that proof directly alongside the deterministic benchmark-row contract test.

The seeded shell test remains useful only for locking the canonical row IDs, coarse metrics, and retained-hotspot formatting. It no longer carries the burden of proving shipped-surface baseline truth by itself. On that basis, both `PERF-01` and `PERF-02` are satisfied and the phase goal is achieved.

Residual notes:

- The real-surface timings are live measurements, so exact microsecond values vary slightly per run; the current rerun stayed consistent in stage ordering and non-zero coverage but did not exactly match the previously recorded sample.
- `cargo fmt --check` still fails on unrelated workspace drift outside Phase 26.

---

_Verified: 2026-05-11T04:06:59Z_
_Verifier: the agent (gsd-verifier)_
