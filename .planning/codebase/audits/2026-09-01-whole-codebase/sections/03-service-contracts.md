# Section 03 — Service contracts

## Scope and coverage

Reviewed all 9 assigned files: the service crate manifest and five Rust
modules, plus the three shipped interface manifests. Callers and consumers in
the module graph, backend/scripting runtime, shell service router, frontend
host/compiler, CLI, and LSP were searched. **9/9 assigned files inspected; no
follow-up remains.**

## Process tree

```text
module.json contract / provider declaration
  -> parse, canonicalize, compile types and operation policy
  -> graph compatibility and provider selection
  -> immutable ResolvedServiceCatalog generation
  -> capability-filtered Luau proxy / backend binding
  -> typed state, event, and method transport
  -> shell validation, optimistic state, completion and observer fan-out
  -> provider health, replacement, cancellation, recovery, teardown
```

The current design has moved toward compiled contracts and an atomic catalog,
but legacy raw contract views, graph selection, shell policy, and runtime
completion state still cross this boundary. The main audit risks are repeated
compilation on builder lookups, incomplete recursive validation, and ensuring
that every state transition is bound to one catalog/provider generation.

## Performance findings

### S03-PERF-001 — Builder lookups rebuild the complete catalog

- **Source:** `crates/core/extension/service/src/interface.rs:279-287`.
- **Current behavior:** `InterfaceCatalog::resolve` calls `self.build()` for
  each lookup, reconstructing bindings, feature negotiation, and fingerprints.
- **Why it matters:** tooling or activation code that retains the mutable
  builder can repeatedly pay catalog work for adjacent imports and provider
  queries.
- **Recommended improvement:** require callers to publish/build once per graph
  generation and resolve against the immutable snapshot; retain the builder
  API only for preparation.
- **Test/benchmark:** catalog with 10/100/1000 contracts/providers and 1/10/100
  repeated lookups; count allocations and compare builder versus snapshot
  p50/p95 latency and total CPU.
- **Confidence:** confirmed behavior; impact unmeasured. **Status:** new.

### S03-PERF-002 — Method dispatch reparses schemas and serializes a budget probe

- **Source:** `crates/core/runtime/scripting/src/context/proxy.rs:187-235,263-270`.
- **Current behavior:** each call reparses every argument's `TypeExpr`, converts
  Lua values to JSON, validates them, and serializes a second JSON envelope just
  to measure output bytes.
- **Why it matters:** high-frequency controls or polling services can turn
  contract parsing and JSON conversion into per-interaction overhead.
- **Recommended improvement:** use the compiled method schema and a typed Lua
  conversion/size accounting path; keep JSON serialization at the transport
  boundary where required.
- **Test/benchmark:** 60 Hz slider/toggle calls with 1, 8, and 32 arguments and
  nested objects; measure allocations, Lua-to-JSON conversions, CPU, and queue
  latency before/after. Do not assume a speedup without this workload.
- **Confidence:** high; unmeasured impact. **Status:** new.

### S03-PERF-003 — Per-field service reads lock and convert independently

- **Source:** `crates/core/runtime/scripting/src/context/proxy.rs:297-305,584-598`.
- **Current behavior:** every Lua field lookup locks `ServiceContextState`, finds
  one field, and converts it to a Lua value; a component reading several fields
  repeats the lock and conversion work.
- **Why it matters:** service-heavy components can contend with publication and
  repeatedly clone shared values during one render/handler turn.
- **Recommended improvement:** expose a generation-stamped immutable per-service
  view for one Lua turn, while preserving read-only semantics and capability
  filtering.
- **Test/benchmark:** 1/10/50 fields read by 100 components while snapshots
  publish at 1/60 Hz; measure lock hold time, allocations, and end-to-end Lua
  handler time, including stale-generation behavior.
- **Confidence:** medium; current locks are confirmed, hot-path impact is a
  hypothesis. **Status:** new.

### S03-PERF-004 — Service fan-out and completion maps need bounded-load evidence

- **Source:** `crates/core/runtime/scripting/src/context/proxy.rs:617-705`,
  shell service state and routing consumers.
- **Current behavior:** call tickets retain completion entries and event queues
  are shared through mutex-protected collections; reclamation depends on later
  completion/cancellation handling.
- **Why it matters:** abandoned tickets, slow providers, or many observers can
  increase memory and lock contention without a demonstrated bound.
- **Recommended improvement:** enforce per-context pending-call and event
  budgets, expiry, and observable rejection; make completion retention tied to
  a provider/catalog generation.
- **Test/benchmark:** synthetic 60 Hz calls with provider delays, cancellations,
  and disconnected consumers; measure peak queue/ticket count, RSS, lock
  contention, and failure isolation. **Status:** new hypothesis.

## Dead code and redundancy

### S03-DEAD-001 — Raw and compiled contract models duplicate live schema data

- **Source:** `crates/core/extension/service/src/contract.rs:7-25,120-142`,
  `CompiledContract::to_interface_contract` at `:209-218`.
- **Current behavior:** the compiled artifact retains an `Arc` to the full raw
  contract while also storing cloned state, methods, events, types, policy, and
  behavioral metadata; callers can reconstruct another raw contract.
- **Why it matters:** two representations can diverge in consumers and increase
  memory/clone cost; the compatibility surface makes it unclear which is the
  authority.
- **Recommended improvement:** make the compiled contract the runtime authority
  and isolate raw declarations behind parsing/tooling adapters; remove the raw
  retention only after auditing serialization and LSP callers.
- **Test:** repository-wide consumer search, compile-time API review, snapshot
  parity tests, and memory measurement for 1/100/1000 contracts.
- **Confidence:** high redundancy; not safe to call raw data dead because
  compatibility/tooling uses it. **Status:** new design debt.

### S03-DEAD-002 — Transitional registry APIs remain alongside atomic catalog

- **Source:** `interface.rs:81-148` and builder compatibility methods at
  `:102-119,279-287`.
- **Current behavior:** `replace_catalog` is a compatibility spelling and the
  mutable builder still exposes direct lookup/build paths even though the
  runtime model requires one published immutable generation.
- **Why it matters:** new callers can accidentally resolve a rebuilt or partial
  view rather than the activation snapshot.
- **Recommended improvement:** mark preparation-only APIs distinctly, make
  direct builder resolution internal, and retain only an explicit migration
  adapter until all callers move.
- **Test:** repository-wide call graph plus compile-fail/API deprecation checks;
  ensure shell, LSP, and CLI resolve through the same published handle.
- **Confidence:** confirmed parallel authority; possible external API risk.
  **Status:** related to older Section 03 audit.

### S03-DEAD-003 — Generated service documentation is a second contract projection

- **Source:** `crates/core/extension/service/src/generator.rs:1-124,194-245`.
- **Current behavior:** Luau type declarations and Markdown are independently
  emitted from raw contract fields while runtime dispatch uses compiled schema
  and operation policy.
- **Why it matters:** a projection can advertise a method/return shape that the
  runtime does not accept, especially as optional and named types evolve.
- **Recommended improvement:** generate all authoring and runtime-facing
  projections from one compiled contract artifact, with snapshot tests for
  types, capabilities, and method results.
- **Confidence:** high duplication of projection logic; not dead because tools
  consume the outputs. **Status:** new.

## Logic and core mechanics

### S03-LOGIC-001 — Recursive type declarations are not enforced recursively

- **Source:** `contract.rs:TypeExpr::matches` and compiled schema construction;
  runtime validation in `crates/core/shell/src/shell/runtime/service_state.rs`.
- **Current behavior:** array/named-type checks validate only the outer JSON
  category in the reviewed path; nested member/field types and optional
  presence rules are not uniformly enforced for state, events, method inputs,
  and method outputs.
- **Why it matters:** typed contracts become advisory at the provider boundary,
  so malformed state can reach frontends or a provider can return a shape that
  the contract never promised.
- **Recommended improvement:** compile a recursive schema graph with named-type
  cycle detection and use it for every transport direction; preserve the last
  known-good state and degrade the provider on invalid snapshots.
- **Test:** nested arrays, named records, optional fields, cycles, invalid
  method returns, and malformed event payloads through the full shell path.
- **Confidence:** high. **Status:** related to older Section 03 audit, rechecked.

### S03-LOGIC-002 — Provider selection and contract compatibility need one binding

- **Source:** `interface.rs:305-366`, graph provider selection, and shell
  discovery consumers.
- **Current behavior:** the catalog builds a binding for each contract, but
  graph-selected providers, version negotiation, feature negotiation, and
  frontend requested ranges are supplied by separate callers. The exact
  provider/contract/generation tuple is not yet the sole input to all dispatch
  and launch paths.
- **Why it matters:** a frontend can observe one version while a backend or
  validation path uses another, causing false availability or stale routing.
- **Recommended improvement:** publish one resolved tuple containing contract,
  provider, version, feature result, operation policy, and generation; require
  launch, proxy, validation, and diagnostics to consume it.
- **Test:** two contract versions and two providers with crossing ranges,
  explicit provider selection, replacement, and concurrent lookup; assert every
  consumer sees the same tuple.
- **Confidence:** medium-high; atomic catalog work reduces but does not remove
  the cross-package seam. **Status:** older audit, still open architecture.

### S03-LOGIC-003 — Optimistic state bindings need correlated rollback

- **Source:** `crates/core/shell/src/shell/runtime/service_state.rs:94-118,213-220`
  and shell request routing.
- **Current behavior:** a pending command-bound value overrides provider snapshots
  until an equal value confirms it; the pending record must be correlated with
  completion, provider identity, and generation to distinguish failure, timeout,
  replacement, and newer writes.
- **Why it matters:** a rejected/clamped command or provider switch can leave an
  old optimistic value suppressing authoritative state.
- **Recommended improvement:** key each pending write by call ID, field, prior
  value, desired value, and provider/catalog generation; confirm or roll back
  conditionally on the matching result.
- **Test:** success, rejection, clamp, timeout, cancellation, provider swap,
  and two overlapping writes to one field.
- **Confidence:** high. **Status:** older audit/backlog overlap.

### S03-LOGIC-004 — Provider readiness must follow validated initial state

- **Source:** `crates/core/runtime/backend/src/lib.rs:136-169` and
  `crates/core/shell/src/shell/backend/lifecycle.rs:117-208`.
- **Current behavior:** backend startup reports running before a validated initial
  snapshot is necessarily available; live switching can commit that status and
  stop the old provider before the replacement proves readiness.
- **Why it matters:** consumers can bind to an unavailable provider, and a
  malformed initial state can create a committed-but-unusable service.
- **Recommended improvement:** prepare and validate initial state before commit;
  keep old provider active until the new generation acknowledges readiness, or
  publish an explicit degraded state with rollback.
- **Test:** delayed, malformed, crashing, and successful initial snapshots while
  switching providers; assert no stale events or half-committed catalog.
- **Confidence:** high. **Status:** older audit/backlog overlap.

### S03-LOGIC-005 — Capability policy still has multiple fallback authorities

- **Source:** `contract.rs:94-118`, proxy authorization at
  `proxy.rs:244-261`, and shell operation routing.
- **Current behavior:** compiled operation policy exists, but compatibility and
  shell paths retain naming/fallback conventions and separately derived
  capability checks for imports, events, and commands.
- **Why it matters:** custom interfaces and feature groups can be authorized in
  one layer and rejected in another; a capability grant can outlive the
  catalog generation it was derived from.
- **Recommended improvement:** resolve explicit read, event, and per-method
  capabilities into the catalog binding and enforce only that generation-bound
  policy at proxy creation, delivery, and dispatch.
- **Test:** custom interface names, multiple required/optional capabilities,
  control/read implication, revoked grants, and catalog replacement.
- **Confidence:** medium-high. **Status:** older audit, narrowed to remaining
  cross-layer policy seams.

## Existing backlog or audit overlap

The older Section 03 report and backlog already cover shared service-state
isolation, correlated calls, optimistic writes, catalog coherence, recursive
schemas, provider readiness, and lifecycle recovery. This report does not
reclassify those established items as new. New items are the repeated builder
build, per-call schema/JSON overhead, per-field lock conversion, raw/compiled
and generator projections, and bounded-load measurement gaps.

## Refuted suspicions

- The current proxy marks the proxy table explicitly before consuming `self`
  (`proxy.rs:708-715`); the former “any first table is self” suspicion is not a
  current finding.
- Calls now carry a `call_id` and return a cooperative poll/await ticket
  (`proxy.rs:236-292,617-705`); the older fire-and-forget claim is not repeated.
- `ResolvedServiceCatalogHandle` swaps an immutable `Arc` atomically
  (`interface.rs:81-123`); additive mutable registry replacement is not claimed
  for that handle.
- No rejected performance experiment from `performance-log.md` is repeated;
  all performance items above require fresh workload measurements.

## Tests and benchmarks needed

- Full contract matrix: recursive schemas, optionality, method arity/types and
  returns, event payloads, feature groups, custom capabilities, and generation
  changes.
- Provider switch and service-call state machine tests covering readiness,
  completion, cancellation, stale events, optimistic rollback, and cleanup.
- Catalog build/lookup, proxy field reads, and call-dispatch benchmarks with
  explicit contract/provider counts and repeated release runs; track CPU,
  allocations, locks, JSON conversions, queue bounds, and p95 latency.

## File coverage

**Assigned:** 9/9 files: `crates/core/extension/service/Cargo.toml`, all five
service Rust sources, and `modules/interfaces/{audio,device,shell-ui}/module.json`.
**Inspected:** 9/9. Repository-wide callers were searched but remain assigned
to their owning sections. **Files still needing review:** none.
