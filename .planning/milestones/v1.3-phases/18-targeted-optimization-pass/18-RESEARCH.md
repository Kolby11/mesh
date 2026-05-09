# Phase 18: Targeted Optimization Pass - Research

**Researched:** 2026-05-09
**Domain:** MESH profiling, benchmark proof, and bounded runtime/render optimization
**Confidence:** HIGH

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Phase 18 chooses the optimization target from fresh profiler measurements captured at the start of the phase.
- The default target is the worst measured canonical benchmark or stage, not the safest speculative improvement.
- If two hotspots are close, prefer the one with the largest absolute latency.
- Backend paths are eligible only when the profiler shows material visible frontend impact.
- Phase 18 proves one focused before/after improvement rather than a full before/after comparison for all five scenarios.
- The selected metric must improve by at least 10%.
- Profiling-off behavior, visual output, benchmark contracts, and backend/service semantics must remain unchanged.
- Benchmark scenario ids, payload shape, launch request semantics, and status behavior from Phase 17 must remain stable.
- Rust core must stay generic and must not gain service-specific audio/provider business logic.

### the agent's Discretion
- Choose the exact benchmark command sequence and measurement format.
- Choose the concrete optimization technique after inspecting profiler output.
- Choose focused regression test selectors that prove the optimized path and protect the benchmark/profiling contract.

### Deferred Ideas (OUT OF SCOPE)
- Full before/after comparison for all five canonical scenarios.
- Strict no-regression performance gate across every benchmark scenario.
- Persistent benchmark history, trace export, or replayable profiling sessions.
- Broad visual/UI redesign while optimizing.
- Package/module manifest restructuring and module management work.
</user_constraints>

<architectural_responsibility_map>
## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|--------------|----------------|-----------|
| Fresh baseline capture | Shell debug/profiling runtime | Planning artifact | Baseline must use the existing `mesh.debug` benchmark/profiling contract and be recorded as phase evidence without adding persistence. |
| Hotspot ranking | Shell debug/profiling runtime | Tests/planning artifact | Ranking should derive from existing `ProfilingSnapshot` stage totals/max values and benchmark row metrics. |
| Targeted optimization | Current measured hotspot owner | Shell/render/runtime/backend as indicated by baseline | The owner is evidence-driven; do not preassign backend or renderer work before baseline. |
| Before/after proof | Tests and phase artifacts | Shell debug contract | `OPT-02` needs a numeric proof artifact plus automated guardrails around the changed code. |
| Regression guardrails | Rust shell/component tests | Existing `.mesh` surfaces | Guardrails protect profiling-off silence, benchmark contract shape, visual behavior, and generic backend semantics. |
</architectural_responsibility_map>

<research_summary>
## Summary

Phase 18 should not start by optimizing code. It should first turn the existing live profiling and benchmark data into a phase-local baseline artifact, rank the canonical scenario/stage costs by absolute latency, and choose the highest-latency target. The current code already records shell, surface, and backend timings in `ProfilingRuntimeState`, exposes them through `DebugSnapshot`, and derives the five canonical benchmark rows in `runtime/debug.rs`.

The strongest implementation pattern is a small evidence loop: capture baseline -> implement one bounded optimization against the selected hotspot -> capture post-change numbers -> verify at least 10% improvement and run focused guardrail tests. This avoids adding persistent benchmark infrastructure while still producing durable proof in `.planning/phases/18-targeted-optimization-pass/18-BASELINE.md` and `18-OPTIMIZATION-PROOF.md`.

**Primary recommendation:** Plan Phase 18 as three dependent plans: baseline/hotspot ranking, targeted optimization, and focused before/after proof with regression guardrails.
</research_summary>

<standard_stack>
## Standard Stack

No new libraries are needed.

### Existing Components
| Component | Purpose | Why Use It |
|-----------|---------|------------|
| `ProfilingRuntimeState` | Records bounded shell/surface/backend stage samples | Already enforces debug-only collection and rolling sample capacity. |
| `DebugSnapshot` / `mesh.debug` payload | Exposes profiling and benchmark rows | Existing contract used by the debug inspector and tests. |
| `BenchmarkScenarioId` / `DebugBenchmarkSnapshot` | Fixed canonical benchmark suite | Phase 17 locked stable ids, labels, statuses, and payload shape. |
| Rust test harness | Focused regression and proof tests | Existing repo standard; no E2E/compositor harness exists. |
| Phase artifacts | Baseline and proof records | Durable enough for GSD without adding runtime persistence. |
</standard_stack>

<architecture_patterns>
## Architecture Patterns

### Measurement-Driven Optimization Flow

```text
Enable debug profiling
  -> Run canonical benchmark interactions
  -> Build DebugSnapshot / mesh.debug payload
  -> Extract stage and benchmark metrics
  -> Rank by absolute latency
  -> Choose top hotspot
  -> Optimize only the owning code path
  -> Re-run the same measurement
  -> Record before/after >= 10% improvement
```

### Pattern 1: Phase-Local Baseline Artifact
**What:** Write a planning artifact that records benchmark/stage, metric, before value, selected target, and rationale.
**When to use:** This phase requires evidence but runtime benchmark persistence is out of scope.
**Recommended file:** `.planning/phases/18-targeted-optimization-pass/18-BASELINE.md`

### Pattern 2: Generic Runtime Optimization
**What:** Optimize the measured generic stage owner, not a service-specific code path.
**When to use:** The hotspot lands in render invalidation, restyle/layout, redraw, input handling, or backend delivery.
**Constraint:** Backend work must preserve generic provider/stage attribution and avoid parsing audio payloads in Rust.

### Pattern 3: Focused Proof Artifact
**What:** Record baseline value, post-change value, percentage improvement, commands run, and guardrail test results.
**When to use:** `OPT-02` asks for one measurable improvement, not a full benchmark product.
**Recommended file:** `.planning/phases/18-targeted-optimization-pass/18-OPTIMIZATION-PROOF.md`

### Anti-Patterns to Avoid
- **Optimizing before baseline:** Violates D-01/D-02 and can produce unprovable work.
- **Changing benchmark contracts to make numbers look better:** Violates D-11 and breaks Phase 17 consumers.
- **Adding persistent trace/history infrastructure:** Explicitly out of scope.
- **Audio-specific Rust optimization:** Violates the project rule that service behavior belongs in providers, not core.
</architecture_patterns>

<dont_hand_roll>
## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Benchmark persistence | New history database or trace files | Phase-local markdown baseline/proof artifacts | Persistence is out of scope and would expand the milestone. |
| Benchmark scenario model | New scenario ids or payload shape | Existing `BenchmarkScenarioId` and `DebugBenchmarkSnapshot` | Phase 17 locked the contract. |
| Backend business behavior | Rust audio-specific branches | Existing generic backend provider/stage samples | Preserves MESH architecture. |
| Test harness | New compositor/E2E harness | Focused Rust shell/component tests | Repo already uses private shell helpers and real-surface component tests. |
</dont_hand_roll>

<common_pitfalls>
## Common Pitfalls

### Pitfall 1: Baseline Is Not Fresh
**What goes wrong:** The phase uses stale Phase 17 output or assumed costs.
**Why it happens:** The existing benchmark rows are live/rolling, not persisted.
**How to avoid:** First plan writes `18-BASELINE.md` from a fresh run or deterministic shell test fixture.
**Warning signs:** No artifact records before values and selected metric.

### Pitfall 2: Optimization Changes Visible Behavior
**What goes wrong:** Redraw/restyle savings alter UI output or interaction state.
**Why it happens:** Render invalidation optimizations can skip needed work.
**How to avoid:** Add tests that assert benchmark contracts plus existing visual/component behavior around the touched surface.
**Warning signs:** Plan changes `.mesh` layout/style copy without a Phase 18 need.

### Pitfall 3: Benchmark Contract Drift
**What goes wrong:** Scenario ids/statuses/metrics change while optimizing.
**Why it happens:** The easiest proof path may be to alter reporting rather than improve runtime behavior.
**How to avoid:** Guardrail tests assert stable ids, payload shape, profiling-off rows, and launch request behavior.
**Warning signs:** Edits in `foundation/debug` change public labels/ids without corresponding explicit guardrail.
</common_pitfalls>

<validation_architecture>
## Validation Architecture

Phase 18 validation should sample after every task because optimization work can regress silently.

| Validation Target | Command | Purpose |
|-------------------|---------|---------|
| Formatting | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo fmt --check` | Ensures Rust formatting stays stable. |
| Benchmark contract | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell benchmark` | Protects five scenarios, payload shape, launch routing, and backend correlation. |
| Profiling runtime | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell profiling_` | Protects debug-only collection, rolling samples, stage summaries, and profiling-off behavior. |
| Debug inspector | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell debug_inspector` | Protects visible inspector/benchmark row behavior when UI is touched. |
| Target-specific | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-shell <selector>` | Proves the optimized hotspot and before/after artifact assumptions. |

Required proof artifacts:
- `18-BASELINE.md`: selected benchmark/stage, before value, ranking table, and rationale.
- `18-OPTIMIZATION-PROOF.md`: before value, after value, percentage improvement, touched files, commands run, and guardrail status.
</validation_architecture>

<open_questions>
## Open Questions

1. **Which hotspot wins?**
   - What we know: Phase 18 must pick the worst freshly measured hotspot.
   - What's unclear: The actual largest-latency stage depends on the fresh baseline.
   - Recommendation: Plan 18-01 produces the selection artifact; Plan 18-02 reads it and optimizes that path.
</open_questions>

<sources>
## Sources

### Primary (HIGH confidence)
- `.planning/phases/18-targeted-optimization-pass/18-CONTEXT.md` - locked user decisions for Phase 18.
- `crates/core/shell/src/shell/runtime/profiling.rs` - profiling accumulator and debug-only recording wrappers.
- `crates/core/shell/src/shell/runtime/debug.rs` - benchmark row derivation and `mesh.debug` payload assembly.
- `crates/core/foundation/debug/src/lib.rs` - public debug/profiling/benchmark contract.
- `crates/core/shell/src/shell/runtime/render.rs` - render/present/redraw timing integration.
- `crates/core/shell/src/shell/runtime/mod.rs` and `runtime/request.rs` - runtime update, backend delivery, and request timing integration.
- `crates/core/shell/src/shell/tests.rs` - existing benchmark/profiling proof patterns.
- `.planning/codebase/TESTING.md` - local test commands and conventions.
</sources>

<metadata>
## Metadata

**Research scope:**
- Core technology: Rust shell profiling and benchmark proof.
- Ecosystem: Existing MESH shell/debug/runtime architecture.
- Patterns: phase-local baseline/proof artifacts, focused runtime optimization, guardrail tests.
- Pitfalls: stale baseline, benchmark contract drift, visual behavior regressions, service-specific Rust behavior.

**Confidence breakdown:**
- Existing code seams: HIGH - confirmed from local source.
- Planning structure: HIGH - follows Phase 18 context and prior Phase 17 plan pattern.
- Concrete optimization target: MEDIUM until Plan 18-01 captures fresh baseline.
</metadata>
