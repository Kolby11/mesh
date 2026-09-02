# Section 11 — Luau runtime and sandbox

## Scope and coverage

Reviewed all 59 assigned files in runtime/scripting/backend, sandbox policy,
storage, host contexts, stream/exec paths, backend modules, tests, and runtime
specifications. Shell, frontend, service, module capability, and package
callers were searched. **59/59 assigned files inspected; no follow-up remains.**

## Process tree

```text
module graph + capabilities + SandboxConfig
  -> RuntimePolicy and isolated Luau realm/context
  -> typed host APIs, service proxies, storage, exec/streams
  -> script load/start and callback budget guard
  -> state/events/side effects/command completion
  -> backend health/lifecycle and shell delivery
  -> generation cancellation, child cleanup, durable flush, quarantine
```

## Performance findings

### S11-PERF-001 — Shared per-thread realms trade VM startup for cross-context contention

- **Source:** `scripting/src/pool.rs:10-31` and runtime context creation.
- **Current behavior:** one sandboxed Luau realm is shared per execution thread;
  contexts isolate through `_ENV` and host-owned state.
- **Why it matters:** scripts with many contexts can contend on shared VM/host
  tables and retain memory until the thread realm is released; fresh realms
  would have the opposite startup cost.
- **Recommended improvement:** keep the architecture unless measured; add
  per-thread pool/realm pressure metrics and bounded lifetime/eviction policy.
- **Measurement:** 1/10/100 contexts, 1/60 Hz callbacks, 1/10 MB Lua heaps;
  compare shared/fresh startup, RSS, lock time, callback p95, and cleanup.
- **Confidence:** confirmed sharing, impact hypothesis. **Status:** new.

### S11-PERF-002 — JSON conversion and validation are repeated at each host boundary

- **Source:** `sandbox/src/lib.rs:30-63`, scripting policy and proxy/exec
  call sites.
- **Current behavior:** values are serialized for byte/depth validation and
  converted between Lua/JSON for service commands, state, events, and effects.
- **Why it matters:** high-frequency service or UI calls can spend significant
  time in conversion and allocation.
- **Recommended improvement:** use typed compiled schemas and one bounded
  conversion per boundary; avoid serialization solely for size measurement when
  a typed cost estimator is equivalent.
- **Measurement:** nested payloads at 1/10/64 KiB and 1/10/32 fields at 60/240
  Hz; measure conversion CPU, allocations, bytes, rejected payloads, and p95.
- **Confidence:** high repeated work; impact unmeasured. **Status:** new; do
  not repeat a rejected Lua-table cache without new evidence.

### S11-PERF-003 — Stream overflow and event delivery contend on shared locks

- **Source:** `scripting/src/backend/exec_stream.rs:332-423,537-650` and event
  dispatch paths.
- **Current behavior:** process/pending/overflow maps and queues are mutex
  protected; overflow may pop an older record to reserve a diagnostic record.
- **Why it matters:** many stream lines or subscribers can delay callbacks and
  reorder/drop output under pressure.
- **Recommended improvement:** preserve bounded queues but measure lock hold and
  make overflow/drop policy explicit per stream/generation.
- **Measurement:** 1/8 streams at 100/10k lines/s, queue budgets 32/256/1024;
  measure p95 delivery, drops, queue depth, CPU, and terminal-event order.
- **Confidence:** medium. **Status:** new hypothesis.

## Dead code and redundancy

### S11-DEAD-001 — Public `spawn_stream` is marked dead-code-compatible beside authorized launch path

- **Source:** `backend/exec_stream.rs:458-476`.
- **Current behavior:** the author-facing `spawn_stream` wrapper is `allow(dead_code)`;
  production launch uses `spawn_stream_with_launch_program` after capability
  resolution.
- **Why it matters:** retaining an unguarded-looking entry point can invite
  future bypass of executable authorization and duplicates the launch boundary.
- **Recommended improvement:** make the authorized function the only production
  API; retain a test-only wrapper if needed and add a compile/call-graph guard.
- **Confidence:** possible dead, repository-wide callers required. **Status:** new.

### S11-DEAD-002 — Legacy callback and modern typed registries overlap

- **Source:** backend runtime command/event registry setup and stream compatibility
  views.
- **Current behavior:** explicit immutable command/event registries coexist with
  legacy callback hooks and compatibility event shapes.
- **Why it matters:** policy, payload validation, and lifecycle semantics can be
  implemented twice.
- **Recommended improvement:** make typed registries canonical and isolate legacy
  adapters at the migration boundary; remove after module/script inventory.
- **Test:** shipped backend scripts, dynamic callback names, event ordering, and
  API compatibility tests.
- **Confidence:** medium-high redundancy; **Status:** older audit.

## Logic and core mechanics

### S11-LOGIC-001 — Backend/frontend policy must be one effective runtime policy

- **Source:** `scripting/src/policy.rs:92-215,414+`, `pool.rs:10-31`, and
  backend runtime initialization at `backend/runtime.rs:170-226,288-326`.
- **Current behavior:** both paths install policy and budgets, but backend,
  frontend, storage, exec, and stream adapters each consume portions of the
  policy.
- **Why it matters:** one unaccounted resource or host operation can bypass the
  declared sandbox contract.
- **Recommended improvement:** pass one immutable policy/budget handle into all
  adapters and expose one exhaustion/health transition.
- **Test:** infinite loops, memory, output/queue/event/child/storage budgets,
  backend and frontend parity, cancellation, and quarantine.
- **Confidence:** medium-high seam; current budget tests cover many paths.
  **Status:** older audit/backlog overlap.

### S11-LOGIC-002 — Service state must not cross context capability boundaries

- **Source:** `context/runtime/state.rs:40-70,144-181`, proxy field reads, and
  context `_ENV` setup.
- **Current behavior:** service payloads are copied into capability-authorized
  context state and host globals are documented as context-local; all write/read
  paths need to preserve that invariant under shared realms.
- **Why it matters:** a leaked backing table or stale context can expose or
  mutate another module's service state.
- **Recommended improvement:** keep Rust-owned immutable payloads, lower only
  authorized copies into a context, make service tables read-only, and bind them
  to module/instance/generation.
- **Test:** unauthorized direct global access, cross-context mutation, revocation,
  context reuse, and stale generation.
- **Confidence:** medium-high; current code has explicit isolation comments.
  **Status:** older audit/backlog overlap.

### S11-LOGIC-003 — Backend callback result, side effects, and health need one transaction

- **Source:** `backend/runtime.rs` command/poll handling, scripting lifecycle
  synchronization, and shell service routing.
- **Current behavior:** callback execution can produce state, events, side
  effects, command completion, and lifecycle status through separate channels.
- **Why it matters:** partial application can publish state without its event,
  acknowledge a command before side-effect rejection, or leave health stale.
- **Recommended improvement:** collect/validate one callback result, commit it
  atomically with call/generation identity, and recover by discarding the whole
  candidate on failure.
- **Test:** invalid state/event/effect combinations, command failure, budget
  exhaustion, cancellation, and retry/recovery.
- **Confidence:** high architecture seam. **Status:** older audit/backlog overlap.

### S11-LOGIC-004 — Stream identity and cleanup need generation-aware terminal ownership

- **Source:** `exec_stream.rs:28-67,314-430,537-620`.
- **Current behavior:** streams have stable IDs, budgets, stop channels, and
  terminal events, but process maps retain tasks until reaping and late events
  can race runtime generation shutdown.
- **Why it matters:** output from a prior module/provider incarnation can reach a
  new consumer, or child resources can outlive the owner.
- **Recommended improvement:** reject delivery for stale generations, make
  terminal event exactly-once, await/cancel all tasks during stop, and release
  child/queue budgets on every terminal path.
- **Test:** rapid stop/restart, stream exit during reload, output overflow,
  spawn failure, task panic, and shutdown without Tokio handle.
- **Confidence:** high. **Status:** older audit/backlog overlap.

### S11-LOGIC-005 — Storage and executable authorization must remain scoped to module/instance

- **Source:** backend runtime `StorageScope::backend` at `backend/runtime.rs:185-187`,
  storage manager, executable policy, and host operation registry.
- **Current behavior:** backends receive scoped storage and capability-derived
  executable policy; generic host APIs remain shared across frontend/backend
  adapters.
- **Why it matters:** an identity omission or basename policy fallback can grant
  access across modules or to unintended executables.
- **Recommended improvement:** include module, instance, generation, and trust
  in every scope; authorize canonical executable identity and record decisions.
- **Test:** same-name modules/instances, symlinked executables, revoked caps,
  restart, and cross-scope storage reads.
- **Confidence:** medium-high. **Status:** older audit/backlog overlap.

## Existing backlog or audit overlap

The older runtime audit covers sandbox application, service isolation, lifecycle
cleanup, subscribers, streams, blocking exec, executable capabilities, durable
storage, typed commands/effects, locale state, globals, callbacks, and readiness.
Current code now installs a shared policy, memory/instruction/frame budgets,
bounded queues, stable streams, scoped storage, and backend identity; those
fixed claims are not repeated. New items are bounded-load measurements and the
possible dead wrapper.

## Refuted suspicions

- `RuntimePolicy` installs memory limits and hooks (`policy.rs:414+`), and both
  pool/backend paths use it; “sandbox is metadata-only” is not current.
- Streams now have stable IDs, child budgets, kill-on-drop, and terminal events;
  the old no-identity/unbounded-queue claim is not repeated.
- No rejected VM/cache optimization is claimed as a win.

## Tests and benchmarks needed

- Cross-tier budget/capability isolation, callback transaction, stream lifecycle,
  storage scope, exec authorization, cancellation, restart, and stale-generation
  tests.
- Runtime benchmarks with contexts/payloads/streams/callback rates and explicit
  CPU, allocations, memory, queue depth, terminal latency, and recovery metrics.

## File coverage

**Assigned:** 59/59 runtime/scripting/backend/sandbox files, backend module
entrypoints, fixtures, and runtime contract documents. **Inspected:** 59/59.
Shell/frontend/service callers were searched but belong to their owning sections.
**Files still needing review:** none.
