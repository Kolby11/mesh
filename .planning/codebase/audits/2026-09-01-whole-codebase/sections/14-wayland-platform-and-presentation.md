# Section 14 — Wayland and presentation

## Scope and coverage

Reviewed all 26 assigned files in `mesh-core-wayland` and
`mesh-core-presentation`, protocol adapters, SHM/buffer/popup/input code, tests,
fixtures, and Wayland/presentation specifications. Shell surface, render,
interaction, and surface-policy callers were searched. **26/26 assigned files
inspected; no follow-up remains.**

## Process tree

```text
surface policy + frame/display list + input seat
  -> Wayland globals/seat/surface/popup setup
  -> prepared config and buffer/SHM allocation
  -> damage/region/scale/frame callback commit
  -> compositor acknowledgement/close/input events
  -> shell state, hit testing, repaint scheduling
  -> retry, recreation, teardown, output/seat removal
```

## Performance findings

### S14-PERF-001 — SHM exhaustion and frame-callback delays can retry too aggressively

- **Source:** Wayland/presentation frame and SHM allocation/submit paths.
- **Current behavior:** buffer allocation or callback timeout failures can cause
  immediate retry/repaint scheduling.
- **Why it matters:** compositor pressure can produce a hot loop, high CPU, and
  repeated allocations while no frame can be shown.
- **Recommended improvement:** use bounded backoff, retain damage, and wait for
  release/callback events; expose retry reason and generation.
- **Measurement:** 60/144 Hz with 1/4/16 buffers, injected allocation failures,
  callback delays 16/100/1000 ms; measure retries, CPU, allocations, max frame
  gap, and eventual damage delivery.
- **Confidence:** high behavior, impact workload needed. **Status:** older audit.

### S14-PERF-002 — Region/geometry-only changes can trigger full buffer work

- **Source:** presentation damage/region commit and shell surface update paths.
- **Current behavior:** protocol-only region/size changes share broad repaint or
  commit scheduling with content changes.
- **Why it matters:** panels/overlays can pay buffer work when only exclusive
  zone, input region, or geometry changed.
- **Recommended improvement:** separate protocol-only commits from paint work,
  while preserving ordering and acknowledgement generation.
- **Measurement:** isolated region, geometry, scale, content, and combined
  changes; measure buffer writes, commits, damage pixels, CPU, and p95 latency.
- **Confidence:** medium. **Status:** new hypothesis.

### S14-PERF-003 — Input event conversion/queueing needs bounded-load measurement

- **Source:** Wayland seat/input event dispatch and shell input bridge.
- **Current behavior:** protocol events are converted into owned shell events and
  queued for interaction processing.
- **Why it matters:** motion bursts can accumulate stale events and add input
  latency when frame processing is behind.
- **Recommended improvement:** coalesce motion only where semantic guarantees
  permit, bound queues, and preserve button/key ordering and diagnostics.
- **Measurement:** 60/240/1000 Hz motion with key/button events and 1/10/100
  surfaces; measure queue depth, coalescing, latency, drops, and ordering.
- **Confidence:** medium hypothesis. **Status:** new.

## Dead code and redundancy

### S14-DEAD-001 — Testing backend and production presentation have separate lifecycle models

- **Source:** presentation test backend and Wayland adapters.
- **Current behavior:** recorder/test presentation models commits and callbacks
  without reproducing compositor release, close, scale, and failure lifecycle.
- **Why it matters:** passing tests can leave production-only cleanup/order bugs.
- **Recommended improvement:** derive test backend from the typed presentation
  lifecycle contract and add explicit fault injection; remove redundant recorder
  paths only after fixture migration.
- **Test:** close/dismiss, release, callback timeout, SHM failure, output/seat
  removal, and surface recreation parity.
- **Confidence:** high model duplication; **Status:** older audit.

### S14-DEAD-002 — Input ownership and surface state are duplicated across adapters

- **Source:** Wayland seat/surface structs and shell surface/input state.
- **Current behavior:** seat focus, pointer ownership, popup identity, and surface
  lifecycle are tracked in protocol and shell layers.
- **Why it matters:** stale owners survive removal or a new surface generation.
- **Recommended improvement:** retain protocol handles in Wayland, publish one
  generation-bound shell input snapshot, and remove redundant mutable mirrors.
- **Confidence:** medium-high. **Status:** older audit.

## Logic and core mechanics

### S14-LOGIC-001 — Failed configuration must not be cached as a successful surface

- **Source:** Wayland surface creation/configuration and presentation state.
- **Current behavior:** configuration failure can leave a cached “created” or
  assumed-present state, so later updates target a surface that was not shown.
- **Why it matters:** shell state, damage, and compositor state diverge and
  retries may be skipped.
- **Recommended improvement:** explicit prepared/submitted/acknowledged/shown
  states with last-known-good surface generation and recoverable errors.
- **Test:** configure failure, missing configure, commit failure, recreation,
  and stale callback.
- **Confidence:** high. **Status:** older audit/backlog overlap.

### S14-LOGIC-002 — Compositor close/dismiss must reach shell lifecycle exactly once

- **Source:** popup/surface close event handlers and shell lifecycle callbacks.
- **Current behavior:** compositor close paths can bypass authored lifecycle
  notification or deliver duplicate close/teardown events.
- **Why it matters:** modules leak runtime state or re-open a surface after the
  compositor has removed it.
- **Recommended improvement:** translate every close into one generation-bound
  lifecycle transition and cleanup acknowledgement.
- **Test:** popup close, toplevel close, surface removal during callback, and
  duplicate protocol close.
- **Confidence:** high. **Status:** older audit.

### S14-LOGIC-003 — Popup grabs and identities must be validated against protocol state

- **Source:** popup creation/update/grab paths and protocol-version handling.
- **Current behavior:** click-grab paths can create without the documented grab;
  update logic assumes protocol/version or popup identity conditions.
- **Why it matters:** focus/input routing and compositor acceptance differ from
  the shell's assumed popup lifecycle.
- **Recommended improvement:** negotiate protocol capabilities, validate popup
  identity/generation on every update, and make grab optional only when the
  contract says so with a visible degraded state.
- **Test:** protocol v1/v2/v3, grab success/failure, stale popup, nested popup,
  and compositor dismissal.
- **Confidence:** high. **Status:** older audit.

### S14-LOGIC-004 — Dynamic size and exclusive-zone geometry need one source of truth

- **Source:** Wayland dynamic size resolution, surface policy, render/layout,
  and presentation commit paths.
- **Current behavior:** measured layout, configured size, margins, and exclusive
  zone can be computed by separate layers with different rounding/timing.
- **Why it matters:** panels can reserve the wrong area or oscillate between
  geometry and content sizes.
- **Recommended improvement:** commit a typed geometry snapshot derived from the
  measured root and bind protocol fields/damage to its generation.
- **Test:** dynamic width/height, scale, margins, anchored edges, overlay, and
  exclusive-zone changes with delayed configure.
- **Confidence:** high. **Status:** older audit/backlog overlap.

### S14-LOGIC-005 — Region-only updates require a guaranteed commit and damage policy

- **Source:** input/opaque/exclusive region update and presentation commit paths.
- **Current behavior:** region changes can be recorded without a content damage
  event, leaving no guaranteed protocol commit.
- **Why it matters:** input or reserved regions remain stale until another paint
  happens.
- **Recommended improvement:** treat region state as an independent commit
  dependency and acknowledge it by generation.
- **Test:** region-only update with no paint, close/reopen, failure/retry, and
  output scale changes.
- **Confidence:** high. **Status:** older audit.

### S14-LOGIC-006 — Seat/surface removal must clear all input ownership

- **Source:** Wayland seat removal, pointer/keyboard focus, popup/surface maps.
- **Current behavior:** removing a seat/surface can leave stale focus/capture or
  popup ownership in shell interaction state.
- **Why it matters:** later events route to removed modules or block new focus.
- **Recommended improvement:** generation-bound teardown clears pointer,
  keyboard, grab, focus, popup, and pending callback state atomically.
- **Test:** seat removal during drag/key repeat/popup, surface replacement, and
  late input event.
- **Confidence:** medium-high. **Status:** older audit.

## Existing backlog or audit overlap

The prior Wayland audit covers failed configuration, close paths, creation fields,
popup grabs/versions, dynamic size, region commits, SHM retry, removal cleanup,
silent failures, test lifecycle, and input gaps. Current reports keep those as
overlap and add protocol-only/input workload measurement.

## Refuted suspicions

- No blanket claim is made that all protocol failures are silent; current code
  has structured diagnostics in several adapters.
- No rejected renderer damage/rounding experiment is repeated without a new
  workload and correctness gate.

## Tests and benchmarks needed

- Protocol version/grab, surface state, close, configure, geometry/region,
  damage, SHM, callback, seat removal, input ordering, and recovery matrices.
- Fault-injected presentation benchmarks with surface/buffer/event rates,
  retries, queue depth, CPU, allocations, frame gaps, and shown-generation
  acknowledgement.

## File coverage

**Assigned:** 26/26 Wayland/presentation source, tests, fixtures, and contract
documents. **Inspected:** 26/26. Shell/render/interaction/policy consumers were
searched but belong to Sections 09, 12, 13, and 15. **Files still needing
review:** none.
