# Section 1 — Core foundation contracts: improvement audit

**Audited:** 2026-08-16  
**Scope:** `mesh-core-capability`, `mesh-core-config`,
`mesh-core-diagnostics`, `mesh-core-events`, and `mesh-core-debug`, including
their module, CLI, scripting, backend, and shell consumers.

This is a point-in-time review record, not a second task tracker. Open work from
this audit lives in [`docs/BACKLOG.md`](../../../../docs/BACKLOG.md).

## Logical process map

```text
module.json                         config.toml / settings.json
    │                                           │
    ├─ normalize required/optional grants       ├─ parse raw document
    ├─ resolve module graph                     ├─ validate known namespaces
    └─ install/update review                    ├─ merge profile/instance layers
              │                                 └─ produce effective settings
              ▼                                             │
      effective capability policy  ◄────────────────────────┘
              │
      ┌───────┴────────┐
      ▼                ▼
frontend ScriptContext  backend ScriptContext
      │                │
      ├─ host APIs     ├─ exec/stream/storage/provider APIs
      └─ raw events    └─ provider state/events
              │
              ▼
       typed operation router
              │
      ┌───────┼─────────────┐
      ▼       ▼             ▼
 shell state  service calls  diagnostics/issues
                                  │
                                  ▼
                         versioned debug snapshot
```

The current implementation does not yet have the central “effective capability
policy” or typed operation-router stages shown above. Manifest declarations are
used as runtime grants, while shell requests can enter through both checked
host/service APIs and unchecked raw event channels. Those missing stages are the
main structural cause of the highest-priority defects below.

## Confirmed findings

### 1. Critical — declarations become grants without a durable approval decision

Frontend activation grants every required **and optional** manifest capability
(`crates/core/shell/src/shell/component.rs:1748`). Backend candidate construction
also concatenates both lists (`crates/core/shell/src/shell/backend/candidates.rs:139`).
Install and update review only the required lists
(`crates/tools/cli/src/main.rs:519`,
`crates/core/shell/src/shell/package.rs:391`, and
`crates/tools/cli/src/update.rs:281`), and neither `mesh.lock` nor profiles retain
an approval set.

This contradicts the contract that optional capabilities may be denied
(`docs/spec/01-module-system.md:854`). It is directly exploitable: a module can
declare optional `exec.command`, receive it at startup, and pass the backend
execution guard. Optional `service.packages.control` is also automatically
effective; the package service then accepts caller-supplied `allow_elevated` /
`allow_high` flags, creating a confused-deputy installation path.

Capability classification compounds the problem. `Capability::new` accepts any
string and `privilege_level` defaults unknown names to `Standard`
(`crates/core/foundation/capability/src/lib.rs:8` and `:21`), although the spec
requires unclassified capabilities to be refused
(`docs/spec/01-module-system.md:867`). `CapabilityHandle` claims proof-token
semantics, but no protected API consumes it; the mutable, cloneable
`CapabilitySet` is the real authority.

**Improve it:** add a catalog-backed `CapabilityPolicy` which produces an
immutable `EffectiveCapabilities` value. Persist explicit approvals, validate
them at install, update, startup, and profile switch, fail activation when a
required grant is absent, and deny optional grants by default. Unknown catalog
entries must fail closed. Frontend and backend runtime construction must consume
only this resolved value, never manifest declarations.

### 2. Critical — raw shell events bypass capability-checked host APIs

Every frontend receives `mesh.events.publish`, which accepts an arbitrary
channel and captures its current capability set without authorizing the
operation (`crates/core/runtime/scripting/src/context/runtime/host_api.rs:79`).
The shell router performs capability checks for service-interface commands but
not for recognized `shell.*` operations
(`crates/core/shell/src/shell/service.rs:175`). Those operations include surface
show/hide/toggle/repositioning, popovers, surface roles, debug controls,
profiling, and benchmarks.

The locale path demonstrates a concrete bypass. `mesh.locale.set` correctly
requires `locale.write` (`host_api.rs:145`), but a module can publish
`shell.set-locale` directly and the router creates `CoreRequest::SetLocale`
without checking the source grant (`service.rs:258`). Direct `mesh.popover`
helpers are also exposed without a capability gate. Malformed position margins
silently become zero even though the service contract says malformed arguments
must be rejected (`service.rs:229`; `docs/spec/01-module-system.md:817`).

**Improve it:** define one typed operation registry with payload schema,
required capability, allowed caller/trust constraints, and rejection diagnostic
for every shell/service operation. Route dedicated host APIs, raw events, and
popover helpers through that same authorization function. Unknown, malformed,
unauthorized, and dropped operations should have distinct structured outcomes.

### 3. High — settings validation can accept an unusable value and discard valid siblings

`render.blur.passes` is validated as any unsigned integer
(`crates/core/foundation/config/src/lib.rs:99` and
`config/src/validate.rs:243`) but deserialized as a documented 1–3 `u8`
(`config/src/lib.rs:273`). A value such as `256` passes validation, fails later
deserialization, and makes `resolve_shell_settings` replace the **entire** shell
namespace with defaults (`config/src/settings.rs:285`). Valid sibling values,
such as a tooltip delay or theme, are lost.

Enum acceptance has a related normalization bug. Tooltip position validation
checks `trim()` (`config/src/lib.rs:129`) but preserves the original string;
runtime matching is exact (`crates/core/shell/src/shell/component/tooltip.rs:219`).
Thus `" bottom "` validates but behaves as `auto`.

The local test run also found an existing failure:
`settings::tests::an_unknown_shell_section_is_reported_without_losing_its_siblings`
expects `fonts`, while validation now correctly reports the more precise
`fonts.packs` path (`config/src/settings.rs:616`). This is a stale test/contract
expectation, not evidence that the more precise path is wrong.

**Improve it:** extend field declarations with numeric bounds and canonicalizing
parsers, apply fallback per rejected field rather than per namespace, and derive
runtime validation plus LSP schema from one authoritative declaration. Add
end-to-end tests for range failure, normalization, valid-sibling preservation,
and diagnostic paths.

### 4. High — diagnostics are last-write-wins, duplicate-prone, and lack recovery lifecycle

`healthy`, `degraded`, and `error` overwrite one health value directly
(`crates/core/foundation/diagnostics/src/lib.rs:78`). A warning after an error
can therefore downgrade visible health, while `healthy()` can hide an unresolved
fault. Handler and lifecycle deduplication is based on unstable text or partial
identity, so it cannot represent explicit resolution cleanly.

`DiagnosticsCollector::register` always appends and has no unregister operation
(`diagnostics/src/lib.rs:201`). Profile activation and component mounting can
register the same module repeatedly, while removal paths do not remove the old
records. Snapshots can therefore contain duplicate stale rows and the collector
can grow across profile switches. Module, module-instance, provider, and runtime
identities are not consistently separated.

**Improve it:** store issues by `(module_id, instance_id, issue_code)` with
severity, current details, first/last seen, count, and active/resolved state.
Compute aggregate health from active issues, and make registration/replacement/
unregistration explicit. Return deterministically ordered module aggregates and
optional instance detail.

### 5. Medium — the advertised foundation EventBus is unused and has misleading semantics

`EventBus::publish` returns `Ok(())` when the channel does not exist and ignores
send failures (`crates/core/foundation/events/src/lib.rs:50`), leaving
`ChannelNotFound` and `InvalidPayload` effectively unreachable. Despite being
described as typed, payloads are unrestricted `serde_json::Value`.

The shell stores an `EventBus`, but operational frontend events use a separate
`PublishedEvent` queue, and service events use another broadcaster. No production
publish/subscribe call sites for the foundation bus were found. This gives
callers a public contract whose error and delivery behavior is not the behavior
of the live system.

**Improve it:** choose one canonical event contract. Either integrate the bus
behind the typed operation registry or remove it. Define channel registration,
payload validation, authorization, backpressure, no-subscriber behavior, and
delivery failure explicitly.

### 6. Medium — debug data has two drifting contracts and crosses the foundation seam

`mesh-core-debug` defines broad shell, module-graph, backend, renderer, paint,
cache, benchmark, and overlay state, although Section 1 is intended to remain
shell/render-neutral. The shell then manually converts these DTOs to JSON rather
than serializing one versioned wire model.

That conversion has already drifted: populated invalidation fields
`narrow_path`, `affected_node_count`, and `script_narrow` exist in
`crates/core/foundation/debug/src/lib.rs:236` but are omitted from
`crates/core/shell/src/shell/runtime/debug.rs:1494`. Debug consumers therefore
cannot observe data the runtime records.

**Improve it:** split stable inspection/telemetry DTOs from renderer metrics and
shell overlay/controller state. Derive serialization for a versioned wire DTO,
add snapshot/schema tests, deterministic ordering, and explicit output bounds.

## Additional hardening opportunities

- Make a non-object settings document return a usable default store plus a
  root diagnostic, reserving `Err` for I/O or irrecoverable parse errors.
- Validate module namespaces transactionally through owner-registered schemas;
  retain unknown/uninstalled namespaces only as an explicit lifecycle feature.
- Replace the fixed settings temporary filename with a unique, permission-aware,
  crash-durable atomic-write strategy for concurrent writers.
- Avoid a leading dot in root-level diagnostic locations when the namespace is
  empty (`crates/core/foundation/config/src/validate.rs:77`).
- Treat empty XDG directory variables as absent rather than relative paths.
- Report allocation profiling as active only when the embedding binary has
  installed the counting allocator, not merely when a crate feature is enabled.

## Recommended implementation order

1. Introduce the capability catalog, persisted approvals, activation gate, and
   immutable effective-grant value.
2. Add the typed shell-operation registry and route events/host helpers through
   its authorization and payload validation.
3. Add bounded/canonical settings declarations and field-level fallback; fix the
   stale diagnostic-path test.
4. Replace diagnostics state with keyed active/resolved issues and explicit
   instance lifecycle.
5. Decide the event architecture and remove or integrate the unused EventBus.
6. Split and version debug telemetry, then close the manual JSON parity gaps.
7. Apply persistence, path, schema-generation, and allocation-profiler
   hardening.

## Required regression coverage

- Optional `exec.command` and `service.packages.control` are absent without an
  explicit approval and present only after approval.
- Startup and profile switching refuse modules missing required approvals;
  updates adding optional elevated/high grants require a new decision.
- Unknown capability IDs fail validation rather than becoming `Standard`.
- Unprivileged locale, surface, popover, and debug event requests are rejected;
  approved core modules keep working through the same registry.
- `blur.passes = 256` rejects only that field and preserves valid sibling
  settings; tooltip enum input is either normalized or rejected consistently.
- Error health cannot be downgraded by a warning, and removing a component or
  switching profiles removes its active diagnostic registration.
- Unknown event channels and malformed payloads return explicit outcomes.
- Debug JSON/schema parity covers every public wire field, including narrow
  invalidation data.

## Verification

Static review covered the five foundation packages and their active consumers.
Three independent Luna xhigh passes reconstructed the flow, challenged logical
ordering and feature design, and checked concrete defects; a fourth focused pass
mapped the capability boundary.

Executed locally with `nix develop`:

```text
mesh-core-capability: 2 passed
mesh-core-diagnostics: 6 passed
mesh-core-events: 1 passed
mesh-core-debug: 2 passed
mesh-core-config: 30 passed, 1 failed
```

The config failure is the stale `fonts` versus `fonts.packs` expectation noted
above. No production code was changed by this audit.
