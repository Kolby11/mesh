# Phase 18: Targeted Optimization Pass - Pattern Map

**Mapped:** 2026-05-09
**Status:** Ready for planning

## Closest Analogs

| Planned Artifact / Change | Closest Existing Analog | Pattern to Reuse |
|---------------------------|-------------------------|------------------|
| Baseline and hotspot proof artifact | Phase summaries and verification reports under `.planning/phases/17-*` | Markdown artifact with explicit commands, measured values, decisions, and requirement links. |
| Benchmark/profiling contract guardrails | `crates/core/shell/src/shell/tests.rs` benchmark and profiling tests | Private shell helper tests that toggle profiling, record synthetic samples, build debug snapshots, and assert payload rows. |
| Runtime profiling measurement | `crates/core/shell/src/shell/runtime/profiling.rs` | Record only when profiling is enabled; keep samples rolling and bounded; aggregate shell plus per-surface scopes. |
| Benchmark row derivation | `crates/core/shell/src/shell/runtime/debug.rs` | Derive scenario status/metrics from existing `ProfilingSnapshot`; keep missing-data states visible and non-fatal. |
| Render-stage timing | `crates/core/shell/src/shell/runtime/render.rs` | Use component profiling records plus present/redraw/total render wrappers. |
| Backend attribution | `record_backend_profiling_stage` and `record_backend_state_publish_delivery` | Record generic interface/provider/stage timings; avoid service-specific payload parsing. |

## File Role Map

| File | Role in Phase 18 |
|------|------------------|
| `crates/core/shell/src/shell/runtime/profiling.rs` | Profiling accumulator, snapshot, and debug-only recording guard. |
| `crates/core/shell/src/shell/runtime/debug.rs` | Benchmark row derivation and `mesh.debug` serialization; likely proof/ranking helper home if code support is needed. |
| `crates/core/shell/src/shell/runtime/render.rs` | Render, present, redraw, and total surface render timing integration; likely optimization seam for render hotspots. |
| `crates/core/shell/src/shell/runtime/mod.rs` | Runtime update handling and backend service delivery timing. |
| `crates/core/shell/src/shell/runtime/request.rs` | Request handling timing and backend command dispatch timing. |
| `crates/core/foundation/debug/src/lib.rs` | Stable public debug/profiling/benchmark types; guardrail file, avoid contract drift. |
| `crates/core/shell/src/shell/tests.rs` | Shell-level benchmark/profiling regression tests and hotspot proof helpers. |
| `crates/core/shell/src/shell/component/tests.rs` | Real-surface inspector and component proof if UI-visible behavior is touched. |

## Reusable Test Patterns

### Toggle, Record, Snapshot
Existing tests instantiate `Shell::new()`, call `CoreRequest::ToggleDebugProfiling`, record shell/surface/backend stages, then inspect `build_debug_snapshot()`.

Use this pattern for:
- baseline extraction fixtures
- before/after proof tests
- profiling-off guardrails
- benchmark contract stability

### Contract Row Assertions
Existing benchmark tests find rows by `BenchmarkScenarioId`, assert status labels, metrics, target identity, and JSON payload shape.

Use this pattern for:
- preserving the five canonical ids
- preserving `Profiling off`, `Waiting for samples`, `Complete`, and `Unavailable`
- proving optimized code did not alter the benchmark API

### Generic Backend Proof
Existing backend correlation tests use `mesh.audio` and provider ids only as generic identities. They do not parse service payload fields.

Use this pattern for:
- any backend-hotspot optimization
- proving backend changes do not add audio-specific Rust behavior

## Constraints for Planner

- Plan 18-01 must produce a fresh baseline artifact before any optimization task.
- Plan 18-02 must read `18-BASELINE.md` and optimize only the selected hotspot.
- Plan 18-03 must produce `18-OPTIMIZATION-PROOF.md` and verify at least 10% improvement.
- Do not modify benchmark ids, labels, launch request names, or JSON payload shape except to add internal helper code that preserves public output.
- Do not introduce persistent benchmark storage.
