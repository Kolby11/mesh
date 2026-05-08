# Phase 14: Profiling Data Model and Timing Hooks - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-05-08
**Phase:** 14-Profiling Data Model and Timing Hooks
**Areas discussed:** Metrics shape, Stage boundaries, Surface accounting, Collection trigger

---

## Metrics shape

| Option | Description | Selected |
|--------|-------------|----------|
| Rolling aggregates only | Current totals, averages, maxima, redraw counts, and latest values only. | |
| Rolling aggregates plus a short recent-sample ring | Aggregates plus bounded recent samples for recent activity inspection. | ✓ |
| Event-oriented recent samples first | Recent-event-centric model with lighter aggregate summaries. | |

**User's choice:** Rolling aggregates plus a short recent-sample ring.
**Notes:** Recent samples should use a fixed sample count per metric bucket. The sample model should be compact by default with a small optional context set instead of rich trace-style event payloads.

---

## Metrics shape retention

| Option | Description | Selected |
|--------|-------------|----------|
| Fixed sample count per metric bucket | Deterministic bounded memory and simple tests. | ✓ |
| Fixed time window | Retain recent seconds of activity with variable sample count. | |
| Hybrid | Retain up to N samples while also evicting by age. | |

**User's choice:** Fixed sample count per metric bucket.
**Notes:** The user preferred deterministic bounded retention over time-window semantics for the first pass.

---

## Metrics shape context richness

| Option | Description | Selected |
|--------|-------------|----------|
| Pre-aggregated stage records | Compact stage records only. | |
| Richer event entries | Heavier event records with more context in every sample. | |
| Split model | Compact records by default plus a few optional context fields. | ✓ |

**User's choice:** Split model.
**Notes:** Optional context should stay practical rather than aggressive.

---

## Metrics shape optional context

| Option | Description | Selected |
|--------|-------------|----------|
| Minimal context | Stage, timestamp/order, duration, surface id, redraw count only. | |
| Practical context | Minimal fields plus small trigger tags and stable surface/module identity when known. | ✓ |
| Aggressive context | Also include request names, command names, provider ids, and similar fields immediately. | |

**User's choice:** Practical context.
**Notes:** The user explicitly preferred small tags such as `input`, `service_update`, and `rebuild`, without turning Phase 14 into trace infrastructure.

---

## Stage boundaries

| Option | Description | Selected |
|--------|-------------|----------|
| Strict shell pipeline stages only | Track only the required top-level buckets. | ✓ |
| Shell pipeline stages plus a few internal sub-stages | Add some lightweight sub-stage detail under the top-level buckets. | |
| Deep stage tree | Nested parent/child timing tree from day one. | |

**User's choice:** Strict shell pipeline stages only.
**Notes:** The required top-level stage list should remain explicit and stable in the first model.

---

## Stage boundaries total surface span

| Option | Description | Selected |
|--------|-------------|----------|
| First-class explicit span | Record total surface render time directly. | ✓ |
| Derived only | Compute total surface render from component stages later. | |

**User's choice:** First-class explicit span.
**Notes:** Total surface render time should be directly measured rather than reconstructed.

---

## Stage boundaries global vs surface

| Option | Description | Selected |
|--------|-------------|----------|
| Separate shell-wide and per-surface records | Track global work and surface-specific work independently. | ✓ |
| Surface-first only | Force all work into surface buckets. | |
| Global-first only | Delay per-surface detail. | |

**User's choice:** Separate shell-wide and per-surface records.
**Notes:** Shell-global work should remain visible even when it does not map neatly to one surface.

---

## Stage boundaries runtime update

| Option | Description | Selected |
|--------|-------------|----------|
| Around shell-side processing before per-surface render stages | Keep runtime update separate from build/layout/paint. | ✓ |
| Fold into tree build/render-related stages | Fewer categories but less explicit orchestration timing. | |
| Split runtime update into multiple required top-level buckets | More taxonomy immediately. | |

**User's choice:** Shell-side processing before per-surface render stages.
**Notes:** Runtime update should remain a visible top-level stage.

---

## Stage boundaries redraw count

| Option | Description | Selected |
|--------|-------------|----------|
| First-class metric | Track redraw count explicitly beside timings. | ✓ |
| Sample metadata only | Keep redraw info attached only to timing samples. | |

**User's choice:** First-class metric.
**Notes:** This was chosen because redraw count is called out directly in milestone scope.

---

## Surface accounting

| Option | Description | Selected |
|--------|-------------|----------|
| Every shell surface the runtime presents | Include panels, popovers, overlays, and other rendered shell surfaces. | ✓ |
| Primary surfaces only | Delay transient/popover surfaces. | |
| Configured allowlist | Only selected proof surfaces count at first. | |

**User's choice:** Every shell surface the runtime presents.
**Notes:** This keeps future proof surfaces like `audio-popover` naturally visible in the model.

---

## Surface accounting hidden surfaces

| Option | Description | Selected |
|--------|-------------|----------|
| Hidden surfaces only appear when they actually render/update | Focus the active model on live work. | ✓ |
| Hidden surfaces remain listed with zeroed or stale entries | Exhaustive but noisier. | |

**User's choice:** Hidden surfaces only appear when they actually render/update.
**Notes:** This keeps profiling output focused and readable.

---

## Surface accounting identity

| Option | Description | Selected |
|--------|-------------|----------|
| Surface id only | Canonical key is just the surface id. | |
| Surface id plus module/component identity when known | Surface id remains canonical with optional labels. | ✓ |
| Module identity first | Group around module ids rather than runtime surface ids. | |

**User's choice:** Surface id plus module/component identity when known.
**Notes:** The runtime unit remains the surface id, but better labels should be attached when available.

---

## Surface accounting repeated renders

| Option | Description | Selected |
|--------|-------------|----------|
| Keep every render sample until the fixed count fills | Raw bounded history. | ✓ |
| Coalesce same-cycle related renders | Cleaner history with more judgment logic. | |

**User's choice:** Keep every render sample until the fixed count fills.
**Notes:** The user preferred raw bounded data over early coalescing heuristics.

---

## Surface accounting shell-wide non-render work

| Option | Description | Selected |
|--------|-------------|----------|
| Keep shell-wide timing even without a visible surface render | Preserve observability for non-render work. | ✓ |
| Only keep data tied to visible surface renders | Hide shell work with no immediate visual effect. | |

**User's choice:** Keep shell-wide timing even without a visible surface render.
**Notes:** The user wanted shell-level non-render work to remain observable.

---

## Collection trigger

| Option | Description | Selected |
|--------|-------------|----------|
| Collect continuously while profiling mode is enabled | Record all relevant hooks until profiling is disabled. | ✓ |
| Collect only when the profiling inspector/view is active | Lower overhead but easier to miss causality. | |
| Hybrid | Cheap counters always, richer samples only with active profiling view. | |

**User's choice:** Collect continuously while profiling mode is enabled.
**Notes:** The user wanted profiling to capture the lead-up to spikes, not only moments when the overlay is being watched.

---

## Collection trigger relationship to debug overlay

| Option | Description | Selected |
|--------|-------------|----------|
| Debug overlay on equals profiling on | Simpler, but all debug sessions pay instrumentation cost. | |
| Separate profiling toggle within the debug path | Profiling stays debug-only but remains explicitly opt-in. | ✓ |

**User's choice:** Separate profiling toggle within the debug path.
**Notes:** The user wanted profiling to stay under the existing debug path without making every overlay session a profiling session.

---

## Collection trigger session reset

| Option | Description | Selected |
|--------|-------------|----------|
| Reset on every enable | Each profiling session starts clean. | ✓ |
| Keep prior session data until explicitly cleared | More history, more chance of stale interpretation. | |
| Reset recent samples but keep lifetime counters | Mixed semantics in the first pass. | |

**User's choice:** Reset on every enable.
**Notes:** The user preferred clean per-session measurements for trust and clarity.

---

## Collection trigger overlay visibility

| Option | Description | Selected |
|--------|-------------|----------|
| Continue collecting until profiling is disabled | Profiling state is independent from overlay visibility. | ✓ |
| Hiding the overlay pauses collection | Lower overhead but loses off-screen profiling opportunities. | |

**User's choice:** Continue collecting until profiling is disabled.
**Notes:** The user wanted to be able to measure interactions without keeping the overlay visible.

---

## the agent's Discretion

- Exact Rust type layout for profiling aggregates, recent sample rings, and optional context representation.
- Exact instrumentation helper locations, as long as the top-level stage contract stays intact.
- Exact enum/string representation for trigger-kind tags and optional identity labels.

## Deferred Ideas

- Full trace persistence and replay.
- Deep nested trace trees or event hierarchies.
- Rich backend-provider/stage attribution beyond lightweight tags in Phase 14.
- Profiling inspector UI and benchmark-specific views, which belong to later phases.
