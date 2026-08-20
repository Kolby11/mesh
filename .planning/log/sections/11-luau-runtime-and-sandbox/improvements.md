# Section 11 — Luau runtime and sandbox: improvement audit

**Audited:** 2026-08-20  
**Scope:** `mesh-core-runtime`, `mesh-core-scripting`, and
`mesh-core-backend`: sandbox policy, Luau realm/context isolation, frontend and
backend lifecycle, host APIs, capability checks, storage, streams, polling,
commands, event publication, cleanup, and diagnostics.

This is an audit record, not a second task tracker. Open implementation work
belongs in [`docs/BACKLOG.md`](../../../../docs/BACKLOG.md). No production code
was changed for this review.

## Logical instruction/process tree

Section 11 is the execution boundary between an installed module and the rest
of MESH. The important unit is not “run a Lua string”; it is a policy-bearing,
stateful service that must preserve isolation across every callback and every
side channel.

```text
installed graph + module manifest + effective capabilities + runtime policy
  │
  ├─ resolve execution tier and resource policy
  │    ├─ Luau vs other tier
  │    ├─ memory and instruction budgets
  │    ├─ allowed host operations and executable policy
  │    └─ failure/quarantine policy
  │
  ├─ create an isolated realm
  │    ├─ sandboxed standard libraries
  │    ├─ per-context `_ENV` / backend module environment
  │    ├─ no shared capability-bearing globals
  │    └─ accounting and cancellation hooks
  │
  ├─ install the typed generic host boundary
  │    ├─ service/interface proxies
  │    ├─ state reads and event subscriptions
  │    ├─ commands and shell/core requests
  │    ├─ storage scoped to module/owner/instance
  │    ├─ bounded exec and stream handles
  │    └─ diagnostics/logging with module identity
  │
  ├─ load source and validate the entrypoint
  │    ├─ parse/compile Luau
  │    ├─ install declarations and imports
  │    ├─ execute top-level source (current behavior)
  │    └─ call explicit `start(self)` / component init
  │
  ├─ run the live callback loop
  │    ├─ poll tick ──► `on_poll(self)`
  │    ├─ command ────► validate/coalesce ─► `on_command_*`
  │    ├─ stream line ─► identify source ──► `on_stream_*`
  │    └─ host event ──► capability gate ──► subscriber callback
  │
  ├─ reconcile one callback result
  │    ├─ convert state/emit payload to typed JSON
  │    ├─ drain interface events and side effects
  │    ├─ publish only declared, authorized updates
  │    ├─ refresh poll schedule and health state
  │    └─ preserve generation/order for shell consumers
  │
  └─ stop, fail, reload, or quarantine
       ├─ cancel callbacks and child processes
       ├─ close subscriptions and bounded queues
       ├─ flush durable storage with diagnostics
       ├─ emit one terminal lifecycle record
       └─ prevent stale state/events/results from re-entering the shell

 feedback loops:
   service update ─► capability-filtered context store ─► script read tracking
                   └► selective frontend invalidation / backend publication
   stream event ──► callback ─► state/event output ─► shell/provider registry
   command ───────► callback ─► result + state settlement ─► caller
   poll failure ──► health/backoff ─► retry or unavailable/quarantine
   source reload ─► cancel old generation ─► new realm/context ─► mount/start
```

### Required invariants

1. Every execution tier uses one authoritative runtime policy. Declared memory,
   CPU/instruction, output, queue, and storage limits are enforced rather than
   remaining metadata.
2. A module cannot read another module’s service payload, storage, globals,
   callbacks, or capability-bearing host handles through a shared Lua realm.
3. Every state read, event subscription/publication, command, executable spawn,
   and shell request is checked against the effective capability and the typed
   contract at the operation boundary.
4. A stream has stable identity, bounded buffering, observable exit/failure,
   and deterministic reaping. A stream callback cannot accidentally merge two
   processes that happen to use the same executable.
5. A callback result is transactional from the shell’s perspective: state,
   command result, events, diagnostics, and health transitions carry one
   generation and cannot be partially or silently applied.
6. Load, start, running, stopping, failed, and stopped are explicit lifecycle
   states. All terminal paths cancel child work, flush storage, and publish one
   truthful terminal outcome.
7. Reload and provider replacement invalidate old generations before any late
   update, event, or command result can reach consumers.
8. Runtime storage is durable by default, securely scoped, bounded, and
   recoverable after process restart or partial writes.

## Verification

- `nix develop -c cargo test -p mesh-core-scripting --lib`: 181 passed, 27
  ignored, 0 failed.
- `nix develop -c cargo test -p mesh-core-backend --lib`: 25 passed, 0 failed.
- `nix develop -c cargo check -p mesh-core-runtime -p mesh-core-scripting
  -p mesh-core-backend`: passed.
- `nix develop -c cargo test -p mesh-core-shell service`: 102 passed, 14
  ignored, 0 failed (the filter also exercised the relevant service-state,
  lifecycle, and integration tests).
- The focused tests cover normal polling, commands, storage, event delivery,
  and basic stream reads, but do not cover the sandbox/resource/lifecycle
  boundaries below. The plain `cargo` command is unavailable outside the Nix
  development shell; all verification above ran inside that shell.

## Confirmed findings

### 1. P0 — Sandbox policy is metadata-only, and backend Luau is not sandboxed

`crates/core/runtime/sandbox/src/lib.rs:8-64` defines `SandboxConfig`,
`ExecutionTier`, and `ModuleRuntime`, but the section source roots contain no
consumer that applies `memory_limit` or `frame_budget_us`. The backend runtime
creates `Lua::new()` directly at
`crates/core/runtime/scripting/src/backend/runtime.rs:139-157`; the frontend
pool is the only path that calls `sandbox(true)`
(`crates/core/runtime/scripting/src/pool.rs:3-13`).

This creates two different trust models inside one “sandbox” section: frontend
realms are frozen Luau sandboxes, while backend realms do not use the same
initialization policy. Neither side enforces the declared memory or CPU budget,
so an extension can consume unbounded execution time, Lua heap, callback
output, or child-process resources.

**Improve it:** introduce one runtime-policy application path used by both
frontends and backends. Create the correct isolated environment first, enable
the Luau sandbox, install instruction/time and memory accounting, and make
budget exhaustion a recoverable module failure with quarantine/backoff. Keep
`SandboxConfig` out of the metadata-only layer by passing an immutable policy
handle into the actual context. Add frontend and backend tests for standard
library mutation, infinite loops, allocation pressure, and budget-triggered
cleanup.

### 2. P1 — Service payloads cross capability boundaries through shared globals

`crates/core/runtime/scripting/src/context/runtime/state.rs:54-70` publishes
each service payload as `__mesh_svc_<name>` on `lua.globals()`. The proxy reads
that global in `crates/core/runtime/scripting/src/context/proxy.rs:368-377`,
while the per-context environment falls through to the shared globals table at
`crates/core/runtime/scripting/src/context/runtime/context.rs:327-342`.

The same thread realm can therefore retain payloads across contexts, and a
script that discovers the magic global can bypass the typed interface proxy.
This is especially serious for a context without the service read capability:
capability filtering at `require()` does not protect a payload already placed in
the shared global table. It also gives one context an ambient mutation surface
for state intended to be Rust-owned.

**Improve it:** remove `__mesh_svc_*` from Lua globals. Keep payloads in a
Rust-owned, capability-filtered per-context store and let each proxy resolve
only its authorized fields. Keep event delivery separate from state reads, and
attach provider/generation metadata to the snapshot. Add same-realm tests that
attempt direct magic-global access, cross-context reads, mutation, and
event-only access without a read grant.

### 3. P1 — Backend failure paths can skip cleanup and terminal lifecycle

`crates/core/runtime/backend/src/lib.rs:126-149` returns immediately on load or
initialization failure. Those paths do not call `kill_streams()`, flush backend
storage, or emit the same `Stopped` terminal record used by the running loop.
Top-level source execution can already invoke host APIs before `start(self)`, so
a script that starts a stream and then fails can leave work behind before the
normal loop cleanup is reached. `BackendScriptContext` has no `Drop` implementation
that compensates for this. Even the normal `call_stop()` path at
`crates/core/runtime/scripting/src/backend/runtime.rs:203-226` flushes storage
only after `stop(self)` succeeds. On a stop error, durable writes can be lost.

Frontend `unmount` has the same ordering weakness:
`crates/core/runtime/scripting/src/context/runtime/lifecycle.rs:155-191` flushes
only after a successful handler, although `ScriptContext::Drop` eventually
flushes if the context is actually destroyed.

The shell can bypass this graceful path entirely: `stop_backend_runtime()` and
profile-switch abort paths call `slot.task.abort()`
(`crates/core/shell/src/shell/backend/lifecycle.rs:63-105` and
`crates/core/shell/src/shell/profile.rs:879`), while `call_stop()` is reached
only by the normal backend loop. An abort can therefore skip `stop(self)`,
stream termination, and storage flushing even after the provider has started.

**Improve it:** centralize finalization in an idempotent guard/state machine.
Every load, start, loop, channel-close, callback-error, and reload path should
run cancellation, stream termination, storage flush, subscription cleanup, and
terminal publication exactly once. Flush in a finally-style path even when the
user stop hook fails. Include lifecycle phase, generation, and failure reason
in the terminal event. Add tests for top-level load failure after stream spawn,
start failure after stream spawn, stop-hook failure with dirty storage, and
channel closure during each phase.

### 4. P1 — Event subscribers after an unsubscribe can be silently skipped

Frontend and backend event channels append subscribers using
`raw_len() + 1` and iterate with `sequence_values()`:

- `crates/core/runtime/scripting/src/context/proxy.rs:257-320`
- `crates/core/runtime/scripting/src/backend/runtime.rs:759-802`

`mlua` sequence iteration stops at the first sequence border. If subscriber 1
unsubscribes while subscriber 2 remains, index 1 becomes a hole and later
subscribers are not visited. The current tests cover one subscriber and basic
unsubscribe behavior, but not a hole before a live subscriber.

**Improve it:** use stable subscription IDs plus `pairs()` over integer keys, or
compact the array before every emit. Return a typed subscription object whose
close operation is idempotent and whose lifetime is tied to the context. Add a
two-subscriber regression test for frontend module events, interface events,
and backend self-events.

### 5. P1 — Stream subprocesses have no identity, reaping, or exit semantics

`StreamState` stores every child and reader in a growing vector
(`crates/core/runtime/scripting/src/backend/exec_stream.rs:28-44`). The reader
pushes lines and exits on EOF/error, but it never removes or awaits its
`StreamProcess` (`:107-143`). EOF only emits a generic wakeup; the backend loop
drains an empty queue and continues (`crates/core/runtime/backend/src/lib.rs:171-243`),
so scripts cannot distinguish a healthy idle stream from a dead stream.

The backend groups drained lines by executable name
(`crates/core/runtime/backend/src/lib.rs:174-200`). Two streams launched with
the same program are therefore merged, even if their arguments or source
handles differ. A long-running provider can accumulate exited children,
reader handles, and unbounded pending lines.

**Improve it:** assign a stream ID at registration and return/retain a typed
handle containing program, arguments, status, and generation. Emit explicit
`started`, `line`, `eof`, and `failed` records; await/reap children; remove
finished entries; and make stop cancellation await completion. Bound lines,
line length, total queued bytes, and active streams. Define overflow behavior
(drop/coalesce/restart/quarantine) and add same-program, EOF, stderr/error,
kill, and high-rate stream tests.

### 6. P1 — `mesh.exec` blocks the async backend loop and has no timeout/output budget

`crates/core/runtime/scripting/src/backend/exec.rs:14-16` calls blocking
`std::process::Command::output()` directly from the Luau callback. The callback
is invoked from the async backend service loop
(`crates/core/runtime/scripting/src/backend/runtime.rs:593-611` and
`crates/core/runtime/backend/src/lib.rs:290-357`). A hung or slow executable
therefore blocks polling, command dispatch, and stream handling on that worker.
`output()` also buffers stdout and stderr without a size limit, and no API
provides cancellation or a deadline.

**Improve it:** move process execution behind an async/worker host service with
per-module concurrency, deadline, cancellation, and maximum output limits.
Expose a structured job/stream handle to Luau rather than making the callback
wait on an unbounded synchronous operation. If a synchronous compatibility API
remains, enforce a small bounded timeout and run it outside the runtime worker.
Test a sleeping command, timeout cancellation, output overflow, concurrent
commands, and provider shutdown while a command is running.

### 7. P1 — Executable capabilities are basename-based and over-broad

`crates/core/runtime/scripting/src/backend/exec.rs:19-40` authorizes an
executable by `Path::file_name()` (`exec.foo`) or by the global
`exec.command` capability. This makes different paths and symlinks with the
same basename equivalent. Granting `exec.sh` also permits arbitrary shell
programs through `sh -c`; the capability model does not constrain argv,
resolved path, environment, or working directory.

**Improve it:** resolve the executable before launch, compare against a
graph-authorized allowlist or capability token, and represent argv/environment/
working-directory policy explicitly. Treat shell interpreters as a separate
high-risk capability and reject unapproved shell-style invocation. Add tests
for basename collisions, symlink/path substitution, `sh -c`, and capability
revocation between queued jobs.

### 8. P1 — Default “durable” storage is process-ID scoped temporary storage

`crates/core/runtime/scripting/src/util.rs:17-24` places the default root at
`<tmp>/mesh/runtime-storage/<pid>`, while frontend lifecycle documentation
describes storage as durable. A restart changes the PID and silently loses the
module’s state. The persistence path
`crates/core/runtime/scripting/src/storage.rs:216-247` uses a temporary file and
rename, but does not fsync the file/directory, enforce secure directory/file
permissions, limit document size, or coordinate concurrent writers.

**Improve it:** resolve the default through the shell’s XDG state/data policy,
create a user-only root, bound key/document sizes, and make the write protocol
durable with versioning, lock/transaction semantics, and recovery diagnostics.
Retain explicit test roots for unit tests. Add restart, crash-between-write-and-
rename, permission, oversized-document, long-scope-ID, and concurrent-writer
tests.

### 9. P1 — Backend command dispatch is not a typed transactional operation

`crates/core/runtime/scripting/src/backend/runtime.rs:324-355` and
`:357-407` resolve `on_command_<name>` but also fall back to an arbitrary
top-level function named after the command. This makes public helper functions
implicitly callable. A handler can mutate state and then return a value that
fails JSON conversion; the conversion error is reported without a settled
command result or an explicit rollback/settlement record.

At the orchestration layer, `coalesce_command_batch` collapses all queued
coalescable messages by command name
(`crates/core/runtime/backend/src/lib.rs:21-49`). Commands for different
targets (for example, different devices) can be incorrectly collapsed.

**Improve it:** derive an explicit command registry from the interface
contract, validate typed arguments before invocation, and require explicit
command exports. Include a command correlation ID, deadline/cancellation,
coalescing key, and atomic result/state/event settlement. Preserve a caller-
visible structured failure when result conversion fails. Every accepted command
must receive exactly one correlated terminal result, including runtime death or
timeout. Do not use a generic provider `Failed` lifecycle record as the
terminal result for an ordinary handler failure: the shell currently treats
that status as provider failure. Add invalid-argument, helper-name collision,
multi-target coalescing, conversion-failure, concurrent-call, and stale-
generation tests.

### 10. P2 — Event and side-effect queues are accepted before a single typed gate

`mesh.events.publish` accepts an arbitrary channel and payload and records
capabilities for later consumers (`crates/core/runtime/scripting/src/context/runtime/host_api.rs:79-103`).
Backend `emit_event` similarly appends arbitrary names and JSON payloads to
`pending_events` (`crates/core/runtime/scripting/src/backend/runtime.rs:552-569`).
Static graph diagnostics and shell routing provide later checks, but the
runtime host boundary itself has no bounded queue or authoritative contract
validation. A failed or slow consumer can therefore accumulate side effects,
and behavior depends on which downstream path drains them.

**Improve it:** inject typed operation handles generated from the active graph;
validate channel/name, payload schema, capability, provider generation, and
queue budget before enqueueing. Make rejection visible to the script and
diagnostics, and bound/coalesce event queues where the contract permits it.
Use one shared operation registry for frontend and backend host calls.

### 11. P1 — Shell-control side effects bypass capability checks

`crates/core/runtime/scripting/src/context/runtime/host_api.rs:79-103`
allows `mesh.events.publish` to queue arbitrary channels, while
`crates/core/shell/src/shell/service.rs:176-235` maps several `shell.*`
channels directly into surface and popover operations. The source capability
metadata is carried in `PublishedEvent`, but the shell path does not enforce a
corresponding operation capability for the surface-control cases. The direct
popover helpers in `host_api.rs:216-340` follow the same pattern. Service
commands have a separate capability check, so the runtime has inconsistent
authorization for equally privileged effects.

The launch path also passes required and optional capability declarations
together (`crates/core/shell/src/shell/component.rs:1748` and
`crates/core/shell/src/shell/backend/candidates.rs:139`), leaving no decision
point at which an optional grant can be denied as required by the module
contract.

**Improve it:** resolve an explicit granted/denied capability decision before
activation, then enforce a typed operation registry at both the Lua host and
shell boundaries. Bind surface operations to the owning module/profile and
reject arbitrary surface IDs. Add actual launch-path tests for denied optional
capabilities, unauthorized show/hide/toggle/promote/popover operations, and
source attribution in diagnostics.

### 12. P2 — Locale state is shadowed by a stale environment-local default

Frontend host installation seeds `__mesh_locale_current = "en"` in the
per-context environment (`crates/core/runtime/scripting/src/context/runtime/host_api.rs:43-49`),
and `mesh.locale.current()` reads that environment table
(`:125-143`). Later service payload application updates the shared global
instead (`crates/core/runtime/scripting/src/context/runtime/state.rs:63-70`).
Because the environment-local value shadows the global, a context can continue
to report `"en"` after the shell changes locale.

**Improve it:** represent locale as an explicit host-owned per-context cell and
update that cell during the same snapshot transaction as translations. Add a
post-initialization locale-switch test that checks `current()`, translation
lookup, reactive invalidation, and fallback behavior together.

### 13. P2 — Public reactive globals cannot be deleted with `nil`

The write synchronization path skips `nil` values
(`crates/core/runtime/scripting/src/context/runtime/sync.rs:95-112`), while
`ScriptState` exposes a setter but no removal operation
(`crates/core/runtime/scripting/src/context/state.rs:59-86`). A script that
changes `visible = "value"` to `visible = nil` can therefore leave the old
value in the Rust-side public state and in downstream rendering/dependency
tracking.

**Improve it:** represent `nil` as an explicit deletion in the write log,
remove the key from `ScriptState`, invalidate dependent template nodes, and
define whether a deleted value falls back to a host prop/default. Add a full
render/update regression, not only a Lua-local assertion.

### 14. P2 — Consumer event channels can forge provider events and isolate poorly

Interface proxies expose both `emit` and `fire`
(`crates/core/runtime/scripting/src/context/proxy.rs:257-320`) to consumers.
That lets a consumer invoke a local callback path that has the same shape as a
provider event, even though provider-owned event publication should be the
only path that reaches the service event bus. Event callbacks are also invoked
inline; one callback error can abort the remaining callbacks in the same
channel.

**Improve it:** expose read-only subscription handles to consumers, reserve
provider publication for a provider-bound capability/handle, and catch/report
each callback failure independently so one subscriber cannot suppress others.
Add provider-versus-consumer ownership tests and a multi-subscriber callback
failure test.

### 15. P1 — Backend callback `self` is recreated, so stateful subscriptions can detach

`BackendScriptContext::current_self_table()` creates a new table on every
`start`, poll, command, and stream callback
(`crates/core/runtime/scripting/src/backend/runtime.rs:708-756`). A backend
that stores `self.events.foo:subscribe(...)` during `start(self)` is therefore
not operating on the same `self.events.foo` table observed by a later callback;
the event channel is reconstructed and its subscription table is different.
The per-call `meta` table is also rebuilt, making identity-based state and
future handle ownership impossible.

**Improve it:** create one runtime-owned backend self/context handle per
generation, preserve stable event/storage/operation handles across callbacks,
and expose immutable metadata separately from per-call input. Add a regression
where `start(self)` subscribes or stores a field and `on_poll(self)` consumes
it, plus a reload-generation identity test.

### 16. P1 — Provider readiness is published before its initial snapshot

The backend loop emits `Started` at
`crates/core/runtime/backend/src/lib.rs:152` before publishing the initial
`start(self)` state at `:164-168`. Shell/provider consumers can observe a
provider as ready and immediately read an empty or old state. Provider
selection also uses synthetic provider identifiers in
`crates/core/shell/src/shell/profile.rs:656`, and inactive-provider updates are
dropped by `service_state.rs:417`, so a startup race can lose the first
authoritative snapshot rather than replaying it after activation.

**Improve it:** stage startup state and events behind a generation, publish the
initial snapshot first, then commit one `Ready`/`Started` record atomically.
Buffer updates until the provider is active, reject stale generations, and add
tests for initial-state visibility, provider replacement, and a startup update
that arrives before selection commit.

### 17. P1 — Reload clears Rust indexes but reuses stale script state

The frontend reload path resets bookkeeping and indexes but does not replace
the existing Lua environment (`crates/core/runtime/scripting/src/context/runtime/context.rs:422-460`).
The full environment teardown exists in `uninit()` but is not part of this
reload path. Old globals, closures, handlers, and public reactive keys can
therefore survive source replacement even after Rust-side registries are
cleared. `ScriptContext::new_for_instance()` also reports module identity in
metadata rather than the component/instance identity at
`crates/core/runtime/scripting/src/context/runtime/lifecycle.rs:217`.

**Improve it:** compile and initialize a fresh environment/context for every
reload, then atomically swap it only after setup succeeds. Preserve only
explicit durable storage and report module, component, instance, and
generation identities separately. Add removed-global, removed-handler,
failed-reload rollback, and metadata-identity tests.

### 18. P1 — Backend source executes top-level side effects before `start(self)`

`BackendScriptContext::load_script()` executes the whole source with
`exec()` at `crates/core/runtime/scripting/src/backend/runtime.rs:163-171`.
Only afterward does `call_init()` require and invoke `start(self)`. Thus a
script can spawn streams, publish events, mutate state, or fail during module
load before its explicit lifecycle entrypoint runs, while the module contract
defines `Loaded` as parsed/validated and the project guidance requires backend
setup inside `start()`.

**Improve it:** separate parse/compile from execution, or execute source in a
fresh staged environment whose side effects are unavailable until `start(self)`
commits. Reject or diagnose top-level host calls and make the lifecycle state
machine distinguish parsed, initialized, and running. Add a test proving that a
top-level `mesh.exec_stream` or event publication cannot escape a failed start.

### 19. P1 — Command/event ingress and JSON payloads are unbounded

Backend command wiring uses `mpsc::unbounded_channel()` at
`crates/core/runtime/backend/src/lib.rs:110-118`; the service loop drains the
entire available batch at `:290-296`. Pending backend events are kept in a
plain `Vec` (`crates/core/runtime/scripting/src/backend/runtime.rs:552-569`),
and service-state snapshots and command results are converted without a
uniform size budget. A burst or malicious payload can consume memory before
coalescing or downstream publication applies backpressure.

**Improve it:** use bounded per-provider queues with explicit overflow and
fairness policy, bound JSON depth/bytes and event count, and account command,
stream, storage, and output budgets under one resource broker. Preserve
correlation IDs when dropping or coalescing work so callers receive a terminal
stale/overflow result instead of waiting forever.

### 20. P2 — `service.*.control` and `service.*.read` privilege semantics diverge

The scripting capability resolver treats a control grant as sufficient for
read/interface access (`crates/core/runtime/scripting/src/host_api.rs:50-119`
and `crates/core/runtime/scripting/src/context/runtime/host_api.rs:404`),
while the shell-side check at `crates/core/shell/src/shell/component/runtime.rs:1490`
recognizes only `.read`. The result is an inconsistent privilege model: a
control-only module may read service state in one path but be denied in another.

**Improve it:** define separate effective read and control grants. Gate state
and events on read, mutations on control, and make any intentional implication
explicit in the contract resolver. Add control-only tests through both the Lua
proxy and shell delivery path.

### 21. P2 — Stream callbacks do not refresh changed poll intervals

Poll and command paths call `refresh_interval()` after a callback, but the
stream path at `crates/core/runtime/backend/src/lib.rs:173-243` dispatches
`on_stream_batch`/`on_stream_line` without refreshing the interval afterward.
If a stream callback changes `self.poll_interval_ms`, the old interval remains
active until the next poll tick, which can be much later than the requested
schedule.

**Improve it:** centralize post-callback scheduling reconciliation for every
callback kind and add a stream-handler interval-change regression.

### 22. P2 — Recoverable backend host setup can panic

Backend host installation uses
`.expect("backend host API setup should succeed")` at
`crates/core/runtime/scripting/src/backend/runtime.rs:146-147`. A setup
failure in a module runtime is converted into a task/process panic instead of
a structured module diagnostic and cleanup path.

**Improve it:** make Lua initialization return `Result<&Lua, BackendScriptError>`
and route all host-installation failures through the lifecycle supervisor. Add
a deliberately failing host setup test that verifies no panic and one terminal
failure record.

## Better feature direction

The current flow treats the Luau context, process runner, provider loop, and
shell router as separate conveniences. A stronger design is a per-module
`RuntimeSession`:

```text
RuntimeSession(policy, generation, capability snapshot)
  ├─ Realm          isolated Luau state + instruction/memory accounting
  ├─ Host           typed capability-checked operation handles
  ├─ ResourceBroker bounded exec/stream/storage jobs
  ├─ Lifecycle      load → start → running → stopping → stopped/failed
  ├─ State          Rust-owned service snapshot + transactional callback output
  └─ Supervisor     health, retry/backoff, cancellation, quarantine, metrics
```

Frontend and backend adapters can keep different callback names, but they
should share this policy, generation, operation, resource, and finalization
model. Providers then become event-driven by default: D-Bus signals, fd/socket
watches, and adopted streams wake the session; polling is a bounded fallback.
The same session can expose a typed command transaction to the shell and a
stable event stream to consumers without relying on magic globals or arbitrary
function discovery.

This direction is intentionally broader than the current implementation flow:
it makes resource exhaustion, stale generations, provider replacement, and
partial callback output first-class states rather than edge cases handled by
individual host functions.

## Recommended implementation order

1. Apply one sandbox/resource policy to every Luau realm; add instruction,
   memory, output, queue, and child-process limits.
2. Move service state out of shared globals and add generation-aware,
   capability-filtered Rust-owned snapshots.
3. Add an idempotent lifecycle supervisor so every early return and callback
   failure cancels children, flushes storage, and emits a terminal record.
4. Replace stream bookkeeping with identity, bounded queues, exit events, and
   awaited reaping; move `exec` off the async worker and add deadlines.
5. Fix sparse event subscription iteration and replace arbitrary command/event
   dispatch with typed contract-backed operation handles.
6. Move default storage to secure durable XDG state, then add restart/recovery
   and concurrency tests.
7. Add health/backoff/quarantine and push-based host primitives, using the
   resulting resource and generation model as the provider ABI.

## Regression matrix

| Area | Regression to add |
| --- | --- |
| Sandbox | backend and frontend stdlib mutation, infinite loop, memory pressure, instruction timeout, and quarantine cleanup |
| Isolation | same-thread contexts cannot read/mutate service payloads, globals, subscriptions, or storage belonging to another context |
| Capabilities | read/control/event/exec grants are independently enforced, including direct Lua access and revoked generations |
| Lifecycle | load/start/poll/command/stream/stop errors all cancel children, flush storage, and emit one terminal state |
| Events | unsubscribe the first of multiple subscribers and verify all later subscribers still run; verify typed name/payload rejection |
| Streams | duplicate programs remain separate; EOF/failure/restart, queue overflow, line limits, child wait, and shutdown are observable |
| Commands | deadlines, cancellation, typed arguments/results, correlation, conversion failure, per-target coalescing, and stale results |
| Storage | restart persistence, secure permissions, atomic recovery, size limits, path limits, and concurrent writers |
| Integration | provider replacement/reload cannot deliver old updates/events/results to a new consumer generation |
