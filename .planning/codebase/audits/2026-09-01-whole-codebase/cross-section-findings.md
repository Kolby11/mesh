# Cross-section findings

## Review scope and execution

This pass joins the completed dated section reports for Sections 01–15 with the
historical Section 16 report and direct source verification. Three fresh
`gpt-5.6-luna` xhigh cross-section agents were dispatched in parallel for
performance, ownership/redundancy, and logic/lifecycle review. Their findings
were reconciled against the source and existing reports. No source fixes were
made.

The cross-section process is:

```text
module.json/profile input
  -> canonical graph and contract/resource snapshots
  -> candidate activation and durable package state
  -> compiler/component tree/Luau contexts
  -> element state, layout, style, interaction, render/display list
  -> Wayland input/configure, damage, SHM, presentation
  -> diagnostics, authoring views, reload/recovery/shutdown
```

## Performance findings

### X-PERF-01 — Independent revision domains still cause broad repeated work

- **Source:** `crates/core/shell/src/shell/runtime/mod.rs:407-481`,
  `crates/core/shell/src/shell/runtime/render/mod.rs:49-110`,
  `crates/core/frontend/render/src/render_object.rs:382-435`, and the
  section reports for 01, 08, 10, 12, and 15.
- **Current behavior:** the shell loop checks and dispatches every domain each
  frame; render startup polls icon/glyph/image work and may request paint for
  every component; retained objects and display-list paths then traverse broad
  trees when dirty signals are not narrow enough.
- **Why it matters:** a service, resource, or control-plane change can compete
  with input and frame deadlines and can turn a local change into repeated
  surface-wide work.
- **Recommended improvement:** publish one immutable frame snapshot with typed
  domain revisions and per-node dirty scopes. Let shell scheduling, element
  layout, render reuse, damage, and presentation consume the same revision
  envelope; keep full rebuilds as an explicit fallback.
- **Test/benchmark:** release profile; 16/128/512 modules, 4/32 surfaces,
  100/1,000 nodes; compare idle, one service field update, theme/locale
  update, resource replacement, and pointer interaction. Record p50/p95/max
  frame time, allocations, traversed nodes, damage area, and queue depth.
- **Confidence:** high behavior; impact remains workload-dependent.
- **Status:** existing backlog and section-audit overlap (`S01-PERF-001`,
  `S08-PERF-001`, `S10-PERF-001`, `S12-PERF-002`, `S15-PERF-001`); no new
  backlog item.

### X-PERF-02 — Blocking package, resource, and extension work shares user-facing paths

- **Source:** package transaction Git/filesystem work in
  `crates/core/extension/module/src/package/transaction.rs:480-690`,
  resource/render loading in `crates/core/frontend/render/src/surface/`, and
  backend command/stream paths in `crates/core/runtime/scripting/src/backend/`.
- **Current behavior:** package staging and fsync are synchronous by design;
  resource cache misses and raster/decode paths can run adjacent to rendering;
  extension process and stream operations have separate worker/budget
  boundaries.
- **Why it matters:** slow disks, Git repositories, large assets, or child
  processes can block activation, frame production, or shutdown and create
  unfairness between modules.
- **Recommended improvement:** keep durable transactions serialized but move
  blocking work to bounded cancellable workers, with explicit byte/time
  budgets, generation-tagged completion, and last-known-good publication.
- **Test/benchmark:** cold/warm package operations with 10/100/1,000 modules;
  cache-miss icon/font/image workloads; backend command and stream bursts;
  inject slow I/O, cancellation, and process exit. Measure shell-frame p95,
  activation latency, worker count, bytes, queue depth, and shutdown bound.
- **Confidence:** confirmed blocking operations; cross-path impact is a
  measurement hypothesis.
- **Status:** existing backlog and section-audit overlap (`S02-PERF-001`,
  `S11-PERF-002`, `S12-PERF-001`, `S15-PERF-002`); no rejected experiment is
  repeated.

### X-PERF-03 — Queue caps are asymmetric and producer backpressure is incomplete

- **Source:** shell message channel and drain in
  `crates/core/shell/src/shell/runtime/mod.rs:361,416`, and Wayland event
  queue/dispatch in `crates/core/shell/src/shell/runtime/wayland.rs:3,78-95`.
- **Current behavior:** producers can enqueue into an unbounded shell channel
  or `VecDeque`, while consumers process only 256 shell messages or 32
  Wayland events per frame; coalescing is batch-local.
- **Why it matters:** sustained IPC, pointer, watcher, or backend bursts can
  grow memory and oldest-event latency while starving lifecycle/control work.
- **Recommended improvement:** use bounded typed queues, latest-value
  coalescing for safe state updates, ordered barriers for lifecycle/IPC, and
  explicit overflow diagnostics.
- **Test/benchmark:** inject 100/1,000/10,000 events above frame capacity;
  measure queue depth, oldest-event latency, coalesced/dropped counts, memory,
  and shutdown responsiveness.
- **Confidence:** medium; impact is workload-dependent.
- **Status:** existing backlog and section-audit overlap (`S14-PERF-003`,
  `S15-PERF-005`); measurement required.

### X-PERF-04 — Service fan-out still clones full JSON payloads before narrow invalidation

- **Source:** `crates/core/shell/src/shell/component/shell_component/mod.rs:349`,
  `crates/core/runtime/scripting/src/context/state.rs:35,73`, and independent
  per-context stores in `context/runtime/context.rs:352`.
- **Current behavior:** a module-wide service update reaches every capable
  runtime and clones the complete payload before field-level change selection.
- **Why it matters:** cost scales with runtime count × payload size and can
  hold runtime locks during fan-out, even when only one field changed.
- **Recommended improvement:** measure shared immutable payload references or
  typed field deltas while preserving Lua context isolation; do not assume a
  table-cache redesign is beneficial.
- **Test/benchmark:** 2KB/50KB payloads, 1/5/50 runtimes, 1Hz/60Hz updates;
  record clone bytes, allocations, lock hold time, rebuilds, and frame p95.
- **Confidence:** high behavior; optimization impact requires measurement.
- **Status:** existing backlog/performance-history overlap; no new speedup
  claim.

### X-PERF-05 — Authoring refresh and profile setup repeat whole-graph work

- **Source:** `crates/core/extension/module/src/package/profile.rs:389`,
  `crates/core/extension/module/src/package/installed_graph/load.rs:109`,
  `crates/core/extension/module/src/package/installed_graph/graph.rs:551,816`,
  and `crates/core/extension/module/src/package/authoring.rs:39-79`.
- **Current behavior:** composition/closure/provider resolution is recomputed
  across profile preflight and graph construction; authoring refresh hashes
  every module tree even for a no-op or one-file change.
- **Why it matters:** startup, profile switching, CLI, LSP, and reload pay
  repeated scans, allocations, semver checks, and synchronous filesystem I/O.
- **Recommended improvement:** pass one immutable activation candidate through
  all consumers and cache module digests by content/metadata identity, with
  safe invalidation for changed roots.
- **Test/benchmark:** 100/500/2,000 modules; 1/10 roots/providers; no-op and
  one-file edits; 10 release/debug runs measuring resolution calls, bytes read,
  hash calls, allocations, and wall/p95 activation time.
- **Confidence:** high repeated work; speedup remains unmeasured.
- **Status:** existing backlog and section-audit overlap (`S02-PERF-003/004`,
  `S15-PERF-002`).

## Duplicated ownership and redundancy

### X-DEAD-01 — Contract, policy, and lifecycle decisions have multiple authorities

- **Source:** graph/manifest validation in
  `crates/core/extension/module/src/manifest/`, service projections in
  `crates/core/extension/service/src/{contract.rs,generator.rs,interface.rs}`,
  compiler/component/LSP analysis, shell package/profile orchestration, and
  CLI package operations.
- **Current behavior:** canonical snapshots exist, but schema projections,
  service documentation, component/import analysis, capability checks, package
  transaction callers, and authoring indexes still contain parallel logic.
- **Why it matters:** a module can be accepted by a tool and rejected by
  activation, or receive different validation, capability, lifecycle, or
  source-provenance behavior depending on entry point.
- **Recommended improvement:** define the canonical owner at each boundary:
  module graph/manifest for module contracts, service contract for typed
  service projections, component AST for authoring syntax, runtime policy for
  capability checks, package transaction for durable mutation, and shell only
  for live orchestration. Generate editor/CLI views from those contracts while
  preserving source spans.
- **Test:** contract parity fixtures run through runtime activation, shell,
  CLI, doctor, and LSP; compare diagnostics, accepted/rejected states,
  provenance, and revisions for malformed and mid-edit inputs.
- **Confidence:** confirmed parallel ownership; some APIs remain externally
  compatible and are not safe to remove immediately.
- **Status:** existing backlog and older-audit overlap (`S02-DEAD-002`,
  `S03-DEAD-003`, `S07-DEAD-001`, `S10-DEAD-002`, `S15-DEAD-001`, historical
  Section 16); no new backlog item.

## Logic and core mechanics

### X-LOGIC-01 — Durable state and live activation still have a post-commit split

- **Source:** `crates/core/shell/src/shell/profile.rs:1990-2160`, especially
  the active-profile/resource/package commit followed by
  `commit_control_plane_batch`; package callers in
  `crates/core/shell/src/shell/package.rs`; CLI callers in
  `crates/tools/cli/src/{main.rs,update.rs}`.
- **Current behavior:** candidate graph, resource snapshot, package journal,
  runtime objects, active-profile pointer, settings, theme, locale, and
  presentation are prepared across related operations. The profile commit
  records a warning when the post-commit control-plane refresh fails, after
  graph/runtime state has already been swapped.
- **Why it matters:** durable profile identity, active runtime generation, and
  rendered settings/theme/locale can describe different generations. A crash
  or ambiguous IPC result can leave recovery with no single authoritative
  committed state.
- **Recommended improvement:** make one activation coordinator own a typed
  candidate containing graph, providers, resources, settings, theme, locale,
  roots, and presentation metadata. Journal prepared/committed/retired
  phases, acknowledge the committed generation, retain the old generation
  until the new one is ready, and make post-commit swaps infallible or enter a
  typed degraded/reconciliation state.
- **Migration cost:** high; it changes shell/profile/package APIs and requires
  crash-injection and recovery compatibility for existing journals.
- **Test:** fail at every durable write, snapshot publication, provider-ready,
  component-register, control-plane, and retirement boundary; restart and
  assert exact pointer, graph, runtime, resources, revisions, and diagnostics.
- **Confidence:** confirmed post-commit error seam; broader crash matrix is
  high-confidence architecture work.
- **Status:** existing backlog/older-audit overlap (`S15-LOGIC-001/002`,
  Section 02 and historical Section 16); no new backlog item.

### X-LOGIC-02 — Candidate graph data can be combined with stale live module identities

- **Source:** candidate graph loading in
  `crates/core/shell/src/shell/profile.rs:856-901`; candidate capability,
  frontend, and backend preparation at `1493-1506,1616-1621`; graph commit in
  `crates/core/shell/src/shell/discovery.rs:2356-2387`; post-install rediscovery
  in `crates/core/shell/src/shell/package.rs:167-176`.
- **Current behavior:** activation loads a fresh canonical graph but prepares
  from `self.modules`. Graph commit does not replace that module map; package
  install masks the seam with explicit rediscovery, while other graph/profile
  activation paths can proceed with copied manifest/path state from an older
  generation.
- **Why it matters:** graph authorization, frontend compilation, capability
  resolution, and backend launch can disagree about module version, kind,
  entrypoint, provider, or requested privileges.
- **Recommended improvement:** derive an immutable candidate module registry
  from the same graph/store snapshot and swap runtime identity records only at
  activation commit; keep lifecycle health separate from manifest identity.
- **Test:** edit module version/kind/entrypoint/capabilities/provider and
  trigger watcher/profile activation; assert every consumer uses one candidate
  manifest and rollback restores the prior identity.
- **Confidence:** high. **Status:** new; add to the shell-core backlog.

### X-LOGIC-03 — `ActiveSnapshot` can lag direct settings/theme/locale commits

- **Source:** snapshot fields in `crates/core/shell/src/shell/profile.rs:315-335`,
  publication at `2199-2218`, and direct control-plane mutation in
  `crates/core/shell/src/shell/runtime/theme.rs:247-290,497-503,595-633,1061-1063`.
- **Current behavior:** profile activation publishes `ActiveSnapshot`, but
  direct control-plane edits update mutable settings/theme/locale state and
  broadcast effects without republishing the snapshot.
- **Why it matters:** public snapshot consumers can observe old revisions and
  settings while components and service payloads use newer state.
- **Recommended improvement:** make one revisioned activation/control-plane
  object authoritative, or republish the snapshot on every successful
  control-plane commit and leave it unchanged on failure.
- **Test:** mutate settings, theme, locale, icon, and font; compare snapshot,
  mutable runtime state, and emitted revisions after success and failed reload.
- **Confidence:** high. **Status:** new; add to the shell-core backlog.

### X-LOGIC-04 — Filesystem graph events can be lost while activation is pending

- **Source:** watcher handling in `crates/core/shell/src/shell/runtime/mod.rs:857`,
  pending-candidate checks in `crates/core/shell/src/shell/profile.rs:669,994`,
  and the main loop at `crates/core/shell/src/shell/runtime/mod.rs:434`.
- **Current behavior:** an event that arrives while resource/backend preparation
  is pending is rejected without retaining the newest graph/content revision;
  pending work is polled, but reconciliation is not guaranteed after commit or
  abort.
- **Why it matters:** an editable manifest/profile change can disappear
  permanently if no later filesystem event arrives.
- **Recommended improvement:** retain the newest canonical authoring revision,
  coalesce events while pending, and retry reconciliation after every candidate
  completion or failure.
- **Test:** delay preparation, send two graph events, complete or abort the
  first candidate, and assert the newest graph becomes active.
- **Confidence:** high. **Status:** new; add to the shell-core backlog.

### X-LOGIC-05 — Generation identity is not yet one envelope across all side channels

- **Source:** provider identity checks in `crates/core/shell/src/shell/profile.rs`
  and `crates/core/shell/src/shell/runtime/mod.rs:660-757`, plus component
  effects, watcher callbacks, restart deadlines, render resources, and
  presentation callback paths.
- **Current behavior:** provider messages and some runtime state carry explicit
  identity/generation checks, while effects, callbacks, resource completions,
  configure/frame events, and teardown use adjacent state and package-specific
  guards.
- **Why it matters:** late work from a retired provider, component, resource
  job, or Wayland object can update a new generation or be mistaken for a
  successful commit; incomplete cleanup can also publish after shutdown.
- **Recommended improvement:** define one typed `ActivationGeneration` plus
  provider/object epochs for every message, callback, deadline, completion,
  cache entry, and presentation object. Reject stale work centrally and make
  terminal cleanup idempotent.
- **Test:** delay each side-channel record across profile/provider/resource/
  surface replacement, reload, and shutdown; assert stale records are dropped,
  no new state changes, and each resource/object is retired exactly once.
- **Confidence:** medium-high; existing checks cover important provider paths
  but not every side channel.
- **Status:** existing backlog and section-audit overlap (`S10-LOGIC-003`,
  `S11-LOGIC-004`, `S12-LOGIC-006`, `S14-LOGIC-002/006`, `S15-LOGIC-003/006`).

### X-LOGIC-06 — Capability enforcement must remain closed across service, UI, IPC, and tools

- **Source:** capability contracts in
  `crates/core/foundation/capability/src/lib.rs`, service proxy paths in
  `crates/core/runtime/scripting/src/context/proxy.rs`, frontend effect/host
  paths in `crates/core/frontend/abi/src/lib.rs` and
  `crates/core/frontend/host/src/lib.rs`, shell IPC and shipped module calls.
- **Current behavior:** effective policy and graph approvals exist, but the
  cross-section reports identify legacy/raw capability fallbacks, shared
  service payload paths, and shell/control effects that do not all converge on
  one proof-bearing runtime gate.
- **Why it matters:** frontends or Luau contexts can observe or request data
  through a route that is weaker than manifest/profile approval, violating the
  capability model and module isolation.
- **Recommended improvement:** resolve capabilities once per committed module
  instance and pass an immutable, non-forgeable authorization context to every
  host API. Keep raw manifest strings and mutable capability sets out of
  protected execution paths; reject absent policy rather than granting a
  fallback.
- **Test:** matrix of frontend/backend/interface/resource/IPC operations with
  absent, optional, required, revoked, and stale-generation grants; assert no
  payload, effect, or tool operation crosses the denied boundary.
- **Confidence:** high for the boundary risk; individual route status is
  covered by the section reports and must be checked before implementation.
- **Status:** existing backlog/older-audit overlap (`S01-LOGIC-004`,
  `S03-LOGIC-005`, `S10-LOGIC-005`, `S11-LOGIC-001/002/011`, historical Section
  16); no new backlog item.

### X-LOGIC-07 — The element-to-presentation path needs one semantic frame contract

- **Source:** component/compiler lowering, `WidgetNode` and layout contracts,
  `crates/core/frontend/render/src/display_list/`, and
  `crates/core/presentation/src/wayland_surface/backend/{damage.rs,present.rs}`.
- **Current behavior:** element state, layout, transforms, style/effects,
  display-list signatures, damage, hit testing, logical/physical extents,
  SHM upload, and compositor commits are represented in neighboring contracts
  with separate rounding, revision, and visibility decisions.
- **Why it matters:** visual output, input targeting, damage, buffer contents,
  and compositor state can disagree for transforms, blur, borders, fractional
  scale, region-only changes, or a configure arriving during interaction.
- **Recommended improvement:** freeze a typed per-frame semantic snapshot with
  cumulative transforms, clips/effect bounds, logical and physical extents,
  resource revisions, stable paint order, interaction visibility, and a
  presentation commit plan. Derive render, hit-test, damage, and SHM/protocol
  operations from that snapshot.
- **Migration cost:** high across elements, render, interaction, surface
  policy, and presentation; introduce parity adapters and golden tests first.
- **Test:** transformed/rotated/shadowed nodes, asymmetric borders, opacity
  groups, blur, fractional scales, region-only updates, interactive configure,
  occlusion, multi-output, and popup promotion; compare pixels, hit targets,
  damage rectangles, and protocol commits.
- **Confidence:** high architecture seam; specific defects are already listed
  in Sections 08, 09, 12, 13, and 14.
- **Status:** existing backlog and older-audit overlap (`S09-LOGIC-001/002`,
  `S12-LOGIC-002/003/004/006`, `S13-LOGIC-003/004`, `S14-LOGIC-004/005`).

## Refuted or bounded suspicions

- The codebase is not missing all activation generations: the shell already
  publishes immutable activation snapshots and checks provider identity in the
  reviewed paths. The finding is that side channels and post-commit control
  plane are not covered by one universal envelope.
- The LSP is not wholly disconnected from canonical graph data: its current
  `ModuleRegistry` uses `AuthoringSnapshot` and refresh generations. The
  remaining concern is parallel schema/projection logic and refresh parity,
  not absence of a snapshot.
- Broad rendering work is not proven to be a universal frame bottleneck. The
  reports identify repeated traversal and blocking paths, but no speedup is
  claimed without the workloads specified above.
- No rejected experiment is repeated as a recommendation. In particular,
  cache, scratch-buffer, and display-list storage changes remain measurement-
  gated by `performance-log.md`.

## Tests and benchmarks needed

- Cross-domain activation failure injection and restart recovery with exact
  generation/pointer/resource/runtime parity.
- Stale side-channel matrix covering providers, Luau effects, resource jobs,
  file watches, Wayland callbacks, and shutdown.
- Capability denial matrix across module, interface, backend, frontend, IPC,
  resource, and tool entry points.
- Contract parity tests for runtime, shell, CLI, doctor, and LSP using the same
  malformed manifests, partial `.mesh` documents, service contracts, and
  settings values.
- Semantic-frame golden tests comparing element/layout, hit testing, display
  list, damage, physical buffers, and protocol commits.
- Performance workloads listed under X-PERF-01 and X-PERF-02, with release
  profile, repeated runs, p50/p95/max, allocations, queue depth, and checked
  gates where one is eventually established.

## Relationship to section reports

This document is a synthesis, not a replacement for the 16 section reports.
Sections 01–15 were already present in the dated audit and were not rewritten;
Section 16 is explicitly reused from its historical planning report. Detailed
file coverage, section-specific refutations, and individual finding IDs remain
in those reports.
