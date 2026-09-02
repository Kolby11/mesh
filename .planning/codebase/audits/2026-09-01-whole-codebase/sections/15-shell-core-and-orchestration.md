# Section 15 — Shell core and orchestration

## Scope and coverage

Reviewed all 134 assigned files in `mesh-core-shell`, shell-owned configuration,
composition manifests, and shell architecture documentation. Discovery,
profile/graph, frontend/backend lifecycle, service routing, resources, reload,
Wayland, package, diagnostics, and shutdown callers were searched. **134/134
assigned files inspected; no follow-up remains.**

## Process tree

```text
Shell::new/config/profile/graph
  -> discovery and candidate catalogs/resources/theme/locale
  -> compile/prepare roots and backend providers
  -> generation-bound activation commit
  -> shell loop: watcher/input/messages/components/effects/commands
  -> layout/paint/presentation and service delivery
  -> reload/profile/package/provider transitions
  -> quiesce, cancel, teardown, flush, stopped
```

## Performance findings

### S15-PERF-001 — Shell-loop work is broad even for small control-plane changes

- **Source:** `shell/runtime/mod.rs:407-481` and profile/discovery reload paths.
- **Current behavior:** each loop checks watcher/reload paths, dispatches input,
  drains messages, ticks components, processes effects, flushes commands,
  renders, and finishes presentation.
- **Why it matters:** unrelated setting/service/file changes can compete with
  input and frame deadlines.
- **Recommended improvement:** retain the bounded loop but schedule typed work
  by dirty domain/deadline and coalesce only semantically safe updates.
- **Measurement:** idle, 60 Hz input, 60/144 Hz frames, 1/100 components,
  service burst, and file burst; measure each stage p50/p95/max, queue depth,
  frame gap, and fairness.
- **Confidence:** high broad path, impact unmeasured. **Status:** new.

### S15-PERF-002 — Profile/catalog preparation can duplicate graph, resource, and UI work

- **Source:** shell/profile preparation and discovery/reconcile callers.
- **Current behavior:** graph, interface catalog, resource catalog, theme/locale
  snapshots, frontend roots, and backend candidates are prepared across several
  stages with owned clones and cross-package indexes.
- **Why it matters:** profile switches and module reloads can reread/clone large
  catalogs and extend user-visible activation latency.
- **Recommended improvement:** use one immutable candidate bundle and share Arc
  substructures; retain rollback ownership and measure clone savings.
- **Measurement:** 10/100/1,000 modules and 10/100/1,000 roots/providers;
  measure bytes read, allocations, preparation wall/p95, and commit latency.
- **Confidence:** medium-high. **Status:** new hypothesis.

### S15-PERF-003 — Debug/profiling snapshots add work to active frames

- **Source:** shell runtime profiling/debug snapshot collection and render loop.
- **Current behavior:** optional diagnostics collect stage/backend/resource and
  runtime summaries during the loop, then clone/format data for consumers.
- **Why it matters:** debugging a frame-sensitive problem can perturb the
  workload and increase per-frame cost.
- **Recommended improvement:** sample/buffer bounded metrics off the hot path;
  defer expensive formatting and make overhead observable.
- **Measurement:** profiling off/on and snapshot intervals 1/10/60 frames with
  100/1,000 components; measure frame CPU, allocations, and diagnostic latency.
- **Confidence:** medium. **Status:** existing backlog-adjacent hypothesis.

## Dead code and redundancy

### S15-DEAD-001 — Shell-owned lifecycle and package policy overlap core contracts

- **Source:** shell package/profile/backend lifecycle modules and core module,
  service, frontend ABI, and runtime contracts.
- **Current behavior:** shell adapters revalidate/route capabilities, provider
  selection, package operations, effects, and lifecycle in addition to lower
  layers.
- **Why it matters:** policy can diverge across shell, CLI, runtime, and graph;
  wrappers obscure the true ownership boundary.
- **Recommended improvement:** keep shell orchestration/state transitions in one
  coordinator and move generic validation/transaction policy to core contracts;
  remove only after call graph and integration tests.
- **Confidence:** high redundancy; **Status:** older audit/backlog overlap.

### S15-DEAD-002 — Legacy enabled-frontend and composition activation paths coexist

- **Source:** shell discovery/profile legacy fallback and composition activation.
- **Current behavior:** configured profiles/compositions and legacy enabled-root
  fallback remain selectable paths.
- **Why it matters:** roots can receive different validation, defaults, resource
  selection, or lifecycle treatment depending on startup mode.
- **Recommended improvement:** make composition/profile the canonical path and
  retain legacy only as an explicit migration adapter with diagnostics.
- **Test:** startup with each mode, root/provider/resource parity, and removal of
  legacy configuration.
- **Confidence:** medium-high. **Status:** older audit.

## Logic and core mechanics

### S15-LOGIC-001 — Profile activation still has a post-commit control-plane failure seam

- **Source:** `shell/profile.rs:1960-2053,2144-2163`.
- **Current behavior:** candidate graph/catalog/resources/pointer are committed,
  runtime objects swapped, then settings/theme/locale control-plane refresh can
  fail and is recorded as a warning after the profile is already committed.
- **Why it matters:** active graph/profile and rendered settings/theme/locale can
  temporarily or permanently describe different generations.
- **Recommended improvement:** include all fallible control-plane preparation in
  the candidate, make commit swaps infallible, or define a typed degraded state
  and reconciliation transaction that cannot report a clean activation.
- **Test:** inject theme/locale/settings publication failure after graph commit;
  assert generation, pointer, runtime, and diagnostics are truthful and recover.
- **Confidence:** confirmed. **Status:** new, related to existing split
  profile/runtime activation backlog.

### S15-LOGIC-002 — Runtime retirement and durable activation remain separate transactions

- **Source:** `profile.rs` activation/retirement paths and package transaction
  integration; `runtime/mod.rs:493-624` shutdown.
- **Current behavior:** durable profile/package commits and live component/provider
  retirement are coordinated by shell follow-up work rather than one durable
  activation record containing every runtime object.
- **Why it matters:** crash or failure between durable commit and retirement can
  reopen a graph that does not match the prior live generation.
- **Recommended improvement:** journal candidate generation and activation
  acknowledgement, retain old generation until new one is ready, and recover
  from an explicit committed/prepared/retired state.
- **Test:** crash/failure at each commit boundary, restart recovery, provider
  switch, component removal, and package rollback.
- **Confidence:** high architecture seam. **Status:** existing backlog/older audit.

### S15-LOGIC-003 — Provider messages are generation-safe, but every side channel must carry identity

- **Source:** `runtime/mod.rs:660-757` and backend message/command/event routes.
- **Current behavior:** provider identity and runtime generation are checked for
  service updates/command results, but shell effects, restart deadlines,
  watcher callbacks, and component completions use additional route state.
- **Why it matters:** a late result from a retired provider can mutate a new
  component or settings state through a side channel not covered by the main
  check.
- **Recommended improvement:** define one `ActivationGeneration`/provider epoch
  envelope for every message, deadline, callback, and completion, rejecting stale
  work centrally.
- **Test:** delayed service/event/command/effect/restart/watcher records across
  profile/provider replacement and runtime reuse.
- **Confidence:** medium-high. **Status:** older audit/backlog overlap.

### S15-LOGIC-004 — Component/runtime errors need failure isolation at loop boundaries

- **Source:** `runtime/mod.rs:407-445` and tick/reload/effect handling.
- **Current behavior:** several fallible operations use `?` from the shell loop;
  one component, reload, or presentation error can abort the run path before
  unrelated modules are isolated.
- **Why it matters:** MESH requires bad extensions to be throttled/quarantined,
  not terminate the whole shell.
- **Recommended improvement:** classify errors by scope, quarantine the failing
  module/surface, retain last-known-good state, and continue with diagnostics;
  reserve process-fatal errors for core/presentation invariants.
- **Test:** one malformed component/backend/theme/resource with independent valid
  modules; assert continued frame/input operation and recovery.
- **Confidence:** medium-high; audit each `?` by error class. **Status:** older
  audit/backlog overlap.

### S15-LOGIC-005 — Bounded message drain does not by itself guarantee request fairness

- **Source:** `runtime/mod.rs:414-439` and recursive effect/request processing.
- **Current behavior:** shell messages are capped at 256 per frame, but each
  message can enqueue more effects/requests that are processed in the same loop
  phase.
- **Why it matters:** a component or provider can create transitive work that
  starves other input, frames, or modules despite the outer cap.
- **Recommended improvement:** apply a shared budget across transitive requests,
  effects, callbacks, and bytes, with continuation scheduling and per-source
  fairness/diagnostics.
- **Test:** recursive request producer plus 60 Hz input/frame and independent
  component; assert bounded latency, queue cap, and eventual progress.
- **Confidence:** high. **Status:** older audit/backlog overlap.

### S15-LOGIC-006 — Shutdown is phased, but detached worker ownership needs explicit join proofs

- **Source:** `runtime/mod.rs:493-624`, watcher/resource preparation workers,
  eventfd wake handles, and backend restart tasks.
- **Current behavior:** shutdown advances through phases and drops the eventfd
  after stopping workers, but all detached task types must prove cancellation,
  join, and no late write before that point.
- **Why it matters:** a late worker can write a closed wake fd, publish into
  cleared state, or keep process resources alive after stopped.
- **Recommended improvement:** store join handles under generation ownership,
  cancel and await each worker, then close shared handles; report timed-out
  cleanup explicitly.
- **Test:** worker blocked at each phase, watcher stop failure, resource job
  cancellation, backend restart race, and repeated/idempotent shutdown.
- **Confidence:** medium-high. **Status:** older audit.

## Existing backlog or audit overlap

The August shell audit covers atomic activation, live enablement, generation-safe
messages, lifecycle/stop/shutdown, workers, errors, watch coverage, bounded
requests, stale provider state, control-plane divergence, startup degradation,
and package transactions. Current code has explicit activation snapshots,
bounded message drains, phased shutdown, and provider identity checks. The new
finding is the verified post-commit control-plane error seam; performance items
are measurement candidates.

## Refuted suspicions

- `runtime/mod.rs:493-624` now has a single phased/idempotent shutdown path;
  “shutdown has no cleanup path” is not current.
- Provider updates/results carry identity/generation checks in the reviewed
  path; a blanket stale-message claim is not promoted without a missed side
  channel.
- No rejected broad traversal/cache optimization is repeated without a new
  workload.

## Tests and benchmarks needed

- Activation commit/failure/crash, post-commit control-plane failure, stale
  side-channel records, scoped error isolation, fairness, shutdown/join, and
  profile/package recovery matrices.
- Shell-loop benchmarks with module/root/provider counts, input/message/effect
  rates, stage timings, allocations, queue depths, and p95/max frame/input
  latency.

## File coverage

**Assigned:** 134/134 shell source/tests, shell-owned config/compositions, and
architecture/crate-boundary documents. **Inspected:** 134/134. Lower-level
package callers were searched but belong to Sections 01–14. **Files still
needing review:** none.
