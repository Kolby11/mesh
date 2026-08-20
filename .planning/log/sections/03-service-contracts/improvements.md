# Section 3 — Service contracts: improvement audit

**Audited:** 2026-08-17  
**Scope:** `mesh-core-service` and the complete service path through installed
graph construction, scripting proxies, backend execution, shell routing,
provider switching, contract validation, compatibility checks, and LSP/update
consumers.

This is a point-in-time review record, not a second task tracker. Open work from
this audit lives in [`docs/BACKLOG.md`](../../../../docs/BACKLOG.md).

## Logical process map

```text
canonical module.json + contract.json + active profile
        |
        v
installed graph
        |- collect interface declarations and provider records
        |- parse one typed contract per retained interface name
        |- diagnose consumer capabilities and provider declarations
        `- select an explicit or sole active provider
        |
        v
mutable InterfaceRegistry / InterfaceCatalog snapshots
        |- contracts sorted by version
        `- all registered providers sorted independently by priority
        |
        +--------------------------+
        |                          |
        v                          v
frontend require/import      backend launch candidate
        |                          |
        |- derived capability      |- graph-selected provider
        |- version lookup          |- start(self)
        `- Luau proxy              `- poll/stream/command loop
        |                          |
        |   method call            |- state snapshot
        |      |                   |- named event
        |      v                   `- command result
        |   queued PublishedEvent          |
        |      |                           |
        +------|---------------------------+
               v
        shell service router
               |- derived control-capability check
               |- method-name check
               |- core handler or active backend queue
               |- optimistic stateBinding write
               `- debug call/result records
               |
        +------+-------------------+
        |                          |
        v                          v
state validation             event validation
warn, then store/deliver      reject invalid, deliver valid
        |                          |
        +------------+-------------+
                     v
       indexed component/runtime delivery
```

The target should collapse the split authority into one generation:

```text
candidate graph
  `- compile canonical contracts
       |- recursive schemas and unique symbols
       |- operation capability policy
       `- compatibility metadata
  `- solve consumer range -> exact contract/provider pair
  `- prepare provider and validate initial state
  `- publish one immutable ResolvedServiceCatalog generation
       |- exact active provider and version
       |- availability/degraded reason
       |- generation-bound frontend proxy
       `- typed, correlated method router
```

## Confirmed findings

### 1. Critical — shared Luau service globals bypass capability and context isolation

Service payloads are lowered into `lua.globals()` under predictable
`__mesh_svc_<service>` keys. The code explicitly notes that per-component
`_ENV` lookup falls through to those globals, so direct reads work
(`crates/core/runtime/scripting/src/context/runtime/state.rs:54-60`). Every
frontend context on a thread shares that Luau realm while treating its `_ENV`
as the isolation boundary
(`crates/core/runtime/scripting/src/context/runtime/context.rs:318-343` and
`context/runtime/vm.rs:59-90`). The authorized proxy reads the same mutable
global table (`context/proxy.rs:368-376`).

An unprivileged component can therefore read `__mesh_svc_audio` without an
audio capability. It can also mutate the shared table, changing what authorized
proxies in other contexts observe until the next provider update. Existing
shared-VM tests grant access to both contexts and do not cover an unauthorized
direct read or cross-context mutation.

**Improve it:** keep canonical service state in a Rust-owned, generation-stamped
store. Give each context only capability-filtered proxy access; never expose the
backing table through shared Lua globals. If a Lua cache remains useful, it must
be private to the authorized context and receive immutable/copy-on-write values.

### 2. Critical — methods are fire-and-forget, not request/response

The public specification calls methods request/response and says declared
argument and return types are strict (`docs/spec/01-module-system.md:293-299`
and `:378-390`). The proxy instead queues a `PublishedEvent` and immediately
returns `{ ok = true, queued = true }`
(`crates/core/runtime/scripting/src/context/proxy.rs:134-175`). Commands,
backend results, and shell messages carry no call ID
(`crates/core/runtime/backend/src/lib.rs:11-19,58-64` and
`crates/core/shell/src/shell/types.rs:324-329`). Actual provider results only
update debug history (`shell/runtime/mod.rs:392-408` and
`shell/runtime/debug.rs:553-577`).

The caller cannot receive its declared return value, distinguish concurrent
same-method calls, observe a rejection, cancel a request, or learn that
coalescing discarded it. The LSP advertises the contract return type while the
runtime returns a queue receipt.

A thrown command-handler error also emits both a failed command result and a
terminal backend `Failed` event
(`crates/core/runtime/scripting/src/backend/runtime.rs:387-405` and
`crates/core/runtime/backend/src/lib.rs:299-317`). The shell stops and supervises
the provider (`shell/backend/lifecycle.rs:218-232`), so one invocation exception
can restart and eventually quarantine an otherwise healthy service.

**Improve it:** add `CallId`, caller/catalog generation, deadline, cancellation,
and a result channel across proxy, shell, and backend records. Return an
awaitable/ticket with `accepted`, `completed`, `failed`, `cancelled`, and
`superseded` outcomes. Separate invocation failure from provider-runtime
failure; a bad call must not terminate the service.

### 3. Critical — failed optimistic writes can pin state forever

The shell applies a method's `stateBinding` after queue acceptance, before the
provider runs the method (`shell/runtime/request.rs:1089-1112`). A pending value
overwrites every conflicting provider snapshot and is removed only when a
snapshot exactly equals the expected value
(`shell/runtime/service_state.rs:94-115,180-213`).

Because command results have no correlation path, failure cannot roll back the
specific binding. Stop, restart, and provider replacement do not clear or scope
pending values to a provider/catalog generation. A rejected setter, a clamped
provider value, a crash, or a provider switch can therefore leave an old
optimistic value permanently authoritative over real provider state.

**Improve it:** make each optimistic write a transaction keyed by `CallId`,
field, provider generation, previous value, and desired value. Confirm it from
the correlated success/state transition; conditionally roll it back on failure,
timeout, cancellation, provider replacement, or stop unless a newer call owns
the field.

### 4. Critical — contract capabilities are not the runtime authorization source

Runtime access is derived from interface naming conventions rather than the
contract's capability declaration. Non-`mesh.*` interfaces are explicitly
allowed without a capability in both lazy and eager import paths
(`crates/core/runtime/scripting/src/host_api.rs:105-123` and
`context/runtime/host_api.rs:404-419,540-555`). Method calls and shell dispatch
invent `service.<projected-name>.control`
(`context/proxy.rs:135-153,403-405` and
`crates/core/shell/src/shell/runtime/request.rs:950-980`). Graph checks that a
frontend declares contract-required capabilities only as diagnostics
(`crates/core/extension/module/src/package/installed_graph/diagnostics.rs:83-110`).

A third-party interface such as `alice.thermal` can declare an opaque consumer
capability yet remain readable with no grant, while a method is guarded by a
different synthesized capability. Required multiple/custom capabilities and
optional capability behavior cannot be expressed authoritatively. The shared
global leak in finding 1 also bypasses even the convention-based read gate.

**Improve it:** compile explicit read, event-subscription, and per-method
operation policy into the resolved binding. Enforce it at activation, proxy
creation, state/event delivery, and dispatch for core and third-party names
alike. Capability implication such as control-implies-read belongs in one typed
policy registry, not in scattered string slicing. This builds on the Section 1
effective-grant work without duplicating the raw `shell.*` finding.

### 5. High — registry resolution is not a coherent view of the active graph

The graph selects the active backend, but the shell registers every provider
and the registry independently chooses a priority-sorted provider
(`crates/core/shell/src/shell/discovery.rs:525-545`,
`shell/backend/candidates.rs:15-38`, and
`crates/core/extension/service/src/interface.rs:58-87`). An ambiguous interface
with no active provider can still satisfy `require` because lookup tests only
whether registry resolution found any provider. An explicitly selected
lower-priority provider can run while the binding reports a different provider.

Contracts and providers are also selected independently. With v1/v2 contracts,
a broad request can select a v2 contract and a higher-priority v1 provider.
No-range lookup bypasses version checks, invalid provider versions fall back to
the selected contract, and a provider can match a versioned request even when
no contract matched (`interface.rs:293-323`). Meanwhile the installed graph
stores declarations/contracts by interface name rather than `(name, version)`,
collapsing the multi-version model before the registry sees it
(`crates/core/extension/module/src/package/installed_graph/graph.rs:25-34,337-348`).

The registry is additive: discovery and graph registration can replace matching
entries but cannot clear, unregister, or atomically swap a generation. Disabled,
removed, or superseded providers/contracts can remain resolvable. Its catalog
snapshot reads contract and provider locks separately, so the halves need not
represent one epoch (`interface.rs:139-157`).

**Improve it:** after the Section 2 fail-closed graph prerequisite, produce one
immutable `ResolvedServiceCatalog` from the enabled compatible graph. Each
binding must contain the exact contract version, selected provider and version,
availability/degraded reason, operation policy, and generation. Atomically swap
the complete snapshot; frontend binding, dispatch, validation, debug state, and
provider launch must consume that same tuple.

### 6. High — declared types are not enforced end to end

`TypeExpr::matches` checks only that an array is an array and a named type is an
object; it never validates array members or named-type fields
(`crates/core/extension/service/src/contract.rs:173-190`). Optional state and
event fields are still diagnosed as missing because validators require every key
(`crates/core/shell/src/shell/runtime/service_state.rs:637-654,724-740`).

Methods receive no arity or type validation. Missing arguments become JSON
`null`, extra arguments disappear, and arbitrary values reach the backend
(`crates/core/runtime/scripting/src/context/proxy.rs:154-170` and
`crates/core/shell/src/shell/runtime/request.rs:1319-1325`). The proxy treats any
first Lua table as a colon-call `self`, so a valid dot call whose first declared
argument is an object/named type silently discards that argument
(`context/proxy.rs:396-401`). Provider return values are not checked against
`InterfaceMethod.returns`.

State violations record warnings and then replace/deliver canonical state,
whereas event violations are dropped
(`shell/runtime/service_state.rs:162-174,434-466`). This makes typed state
advisory while typed events are enforcing. Empty-payload event declarations also
skip payload-shape validation; whether that means unconstrained or no payload is
not specified.

**Improve it:** compile the complete recursive type graph once, including named
fields, arrays, optional presence/null semantics, cycle detection, and a real
`Result` schema. Use it for state, event, method-input, and method-output
validation. Preserve last-known-good state on invalid snapshots and degrade
provider health; make partial state patches an explicit transport mode if they
are needed. Distinguish the actual proxy table from an ordinary table argument.

### 7. High — provider switching commits before service readiness

The backend emits `Started` immediately after script initialization and before
its initial state (`crates/core/runtime/backend/src/lib.rs:136-169`). Live
provider switching commits and stops the old provider on that running status
(`crates/core/shell/src/shell/backend/lifecycle.rs:117-165,187-208`). A process
that initialized but cannot emit a valid snapshot is therefore treated as ready.

Profile preparation uses a synthetic provider identity until commit, then
rewrites the runtime's event identity
(`crates/core/shell/src/shell/profile.rs:654-668,687-724,773-780`). The bridge
copies the current identity into each queued shell message
(`shell/backend/spawn.rs:139-155`). If it forwards `Started` and the initial
update before the shell commits, the update retains the synthetic identity and
can be discarded after commit as inactive.

**Improve it:** buffer candidate output in a prepared-provider object. A
stateful provider becomes ready only after `start`, required host checks, and a
valid initial snapshot; statelessness must be explicit in the contract. Commit
graph, catalog, router, and initial state as one generation, replay buffered
output exactly once, then stop the old runtime. A failed prepare keeps the old
provider and persisted selection unchanged.

### 8. Medium-high — lifecycle edges can leave consumers stale or accept stale events

Provider failure replaces `latest_service_state` with an unavailable snapshot
but does not use the normal delivery path
(`crates/core/shell/src/shell/backend/lifecycle.rs:236-265`). Existing observers
can retain stale healthy state until a later replay/update.

Named interface events reject a mismatched provider only when an active runtime
slot exists. With no slot, delayed events from a stopped or failed provider fall
through to validation and delivery
(`crates/core/shell/src/shell/runtime/service_state.rs:410-448`). State updates
already have a terminal-provider fallback check (`:133-160`).

**Improve it:** make availability a normal generation-stamped service-state
transition delivered immediately to all current observers. Apply the same
active/terminal provider-generation check to state, events, and command results;
discard every old-generation message after stop or commit.

### 9. Medium-high — compatibility checks do not cover the executable contract

`diff_contracts` compares top-level state, methods, events, and newly required
capabilities, but never named type definitions
(`crates/core/extension/service/src/compatibility.rs:59-77`). A `Device` field
can lose or change nested fields while the update is reported compatible.
Changes to `coalesce`, `stateBinding`, interface identity, and version direction
are also invisible even though they alter lossiness and observable state.

The CLI consumes this diff for update refusal, but candidate loading reads only
`module.json`. A string-valued external `contract.json` path is passed to the
inline parser and the error is silently ignored
(`crates/tools/cli/src/update.rs:75-95,204-253`). This external-contract update
gap is also part of the Section 2 transaction/candidate-graph finding.

**Improve it:** diff canonical compiled schemas transitively and in both
directions: consumer compatibility and provider compatibility. Include named
types and behavioral transport annotations, and load external contracts from
the candidate revision before classification. Define the compatibility class
for event payload additions, `coalesce`, and `stateBinding` explicitly.

### 10. Medium — contract identity, authoring, and tooling accept contradictory shapes

Array-form contracts do not reject duplicate or invalid state, method, event,
argument, payload, or named-type field names. Cross-category and reserved proxy
names can collide; runtime and compatibility consumers then choose or collapse
different entries (`contract.rs:223-277,354-446` and
`context/proxy.rs:121-135`). `parse_interface_contract` stores a short interface
name verbatim while lookup canonicalizes it to `mesh.*`, so a registered
`audio` contract cannot resolve as `audio` (`contract.rs:223-225` and
`interface.rs:161-199`).

The spec says external contracts carry descriptions, units, ranges, errors, and
optional feature groups, but the strict parser rejects most of those fields
(`contract.rs:477-556`). Provider identity is forbidden in contract state by
the spec, yet the parser accepts `source_module`, validation special-cases it
away, and the shipped audio contract declares it. Static backend-event analysis
recognizes the legacy `mesh.service.emit_event` form rather than canonical
`self.Event:fire`; LSP manifest schema and registries do not fully model external
or multi-version contracts.

**Improve it:** validate and canonicalize identifiers once; reject duplicate,
reserved, and overlapping symbols; remove runtime metadata from contracts; and
either implement the documented metadata/feature fields or mark them target.
Make graph diagnostics, LSP, generated docs, static Luau analysis, and runtime
consume the same compiled contract artifact.

### 11. Low — the transitional `ServiceRegistry` is a misleading second model

`ServiceRegistry` publicly stores `TypeId -> Arc<Any>`, silently replaces
backends, and maintains separate string metadata
(`crates/core/extension/service/src/registry.rs:23-100`). The shell constructs it
but has no production registration or lookup caller. Its unused
`ServiceError::Conflict` promises behavior registration never returns.

**Improve it:** remove the registry and its unused dependency surface after any
remaining Rust caller is migrated. The generation-stamped contract/provider
catalog should be the only service authority.

## Architectural improvements beyond the current flow

1. Introduce an immutable `CompiledContract` containing canonical identity,
   normalized recursive schemas, unique symbols, operation policy, behavioral
   metadata, schema fingerprint, and declaration provenance. Graph, runtime,
   LSP, update diff, docs, mocks, and generators consume the same artifact.
2. Introduce a generation-stamped `ResolvedServiceCatalog`. It is the sole
   authority for active contract/provider/version/policy/availability and is
   atomically swapped with profile/runtime generations.
3. Make calls first-class transactions. A typed awaitable result, cancellation,
   deadlines, explicit coalescing outcomes, tracing, and optimistic rollback all
   use the same `CallId` rather than separate side channels.
4. Separate consumer and provider compatibility. A new required field may be
   safe for an old consumer but unsafe for an old provider; one undirected label
   cannot express both.
5. Generate Luau consumer types, provider stubs, mocks, and contract
   documentation. Missing handlers, wrong returns, and event payload drift then
   become author-time errors without hardcoding service policy in Rust.
6. Model optional feature groups explicitly and negotiate them per provider, or
   remove the shipped feature-group claim until that protocol exists.

## Recommended implementation order

1. Close the shared-Luau-global service-state leak and add isolation regressions.
2. Land the Section 2 prerequisite: a valid enabled candidate graph with enforced
   module/interface/provider ranges is the only activation input.
3. Add `CompiledContract`: canonical identity, unique symbols, recursive types,
   correct optional semantics, operation policy, and complete compatibility.
4. Replace additive registry mutation with the immutable
   `ResolvedServiceCatalog` and bind proxies/routers to its generation.
5. Enforce contract policy and schemas at import, delivery, dispatch, and result
   boundaries; preserve last-known-good typed state.
6. Add correlated calls, invocation-specific errors, deadlines/cancellation,
   explicit coalescing results, and transactional optimistic state.
7. Gate provider readiness on validated initial state and commit buffered
   provider/profile generations atomically.
8. Complete lifecycle availability/stale-message propagation, tooling/codegen,
   external-contract update checks, and removal of `ServiceRegistry`.

## Required regression coverage

- An unauthorized context sharing a VM cannot read or mutate another context's
  service state through raw globals; authorized proxies remain isolated.
- Custom and core interface reads, subscriptions, and methods fail without the
  exact contract-resolved grants; unknown capability classifications fail
  closed.
- v1/v2 contracts and providers coexist, broad ranges resolve a coherent tuple,
  explicit lower-priority selection wins, ambiguous/unavailable providers do
  not satisfy `require`, and catalog swaps expose one epoch.
- Duplicate/reserved identifiers and invalid provider versions fail contract
  compilation with deterministic provenance.
- Named structures and arrays validate recursively; the missing/null/value
  matrix for `T` and `T?` is consistent across state, events, arguments, and
  returns.
- A dot call with a first object/table argument preserves it; wrong arity,
  extra arguments, and wrong types are rejected before dispatch.
- Two concurrent same-method calls receive their own out-of-order results;
  coalesced calls complete older IDs as `superseded`; timeouts ignore late
  results.
- Handler rejection reaches its caller without stopping the provider. Failed,
  timed-out, clamped, crashed, and provider-switched optimistic writes settle or
  roll back without overriding newer writes.
- Invalid state preserves and delivers the last-known-good/degraded state;
  invalid events are dropped with nested-path diagnostics.
- A provider is not ready before a valid initial snapshot; forced scheduling
  around `Started` and initial update delivers the candidate state exactly once.
- Provider failure immediately delivers `available=false`; stopped/old
  generations cannot publish state, events, or results.
- Compatibility detects nested named-type, `coalesce`, and `stateBinding`
  changes, and external candidate contracts are loaded from the candidate
  revision.
- Static analysis recognizes canonical `self.Event:fire` and proxy subscription
  syntax; LSP external/multi-version contracts and runtime result signatures
  agree with the compiled ABI.

## Verification

Four independent review passes reconstructed the end-to-end service flow,
challenged its logical order and feature model, checked concrete code defects,
and audited contract semantics against the specification and tooling. Existing
performance work on direct registry lookup, validation caching, delivery
indexing, command coalescing, and selective frontend invalidation was treated as
complete and was not re-proposed.

Executed locally with `nix develop`:

```text
mesh-core-service: 30 passed, 5 ignored
mesh-core-shell service_contract slice: 12 passed
mesh-core-shell commands slice: 10 passed
mesh-core-scripting interface_proxy slice: 14 passed, 5 ignored
```

These suites confirm current behavior but do not exercise the failure cases
listed above. No production code was changed by this audit.
