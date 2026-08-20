# Section 15 — Shell core and orchestration audit

**Audited:** 2026-08-20
**Package:** `mesh-core-shell`
**Scope:** startup, discovery, installed graphs and profiles, frontend catalogs,
component and backend runtime coordination, service/request/event routing,
reloads and watch ownership, scheduling, render/presentation orchestration,
live package/profile/provider changes, recovery, and shutdown. No production
code was changed.

Four Luna xhigh passes were used: the requested whole-process instruction-tree
pass, independent logic/order and direct code-error passes, and an additional
transaction/lifecycle specialist. Findings were checked against the current
specification and source. The full shell library baseline ran under
`nix develop`: 666 tests passed, 7 failed, and 130 were ignored. The failures
are the dirty tree's existing fixture, navigation-layout, inspector, theme
contract, backend-manifest, and pixel-equivalence failures; none exercises the
transaction, generation, watcher, effect-budget, or shutdown gaps below.

## Logical process tree

```text
Shell::new
  -> load shell config and shared settings
  -> load active profile and derive effective settings
  -> discover host icon/font resources
  -> resolve initial theme, locale, blur, and watch records
  -> register shell-provided interface contracts/providers
  -> initialize mutable graph/catalog/runtime/presentation maps

Shell::run
  -> discover module manifests
       -> scan configured module roots
       -> parse manifests in parallel
       -> register discovered contracts, providers, icons, and module instances
       -> load the installed graph and add graph contracts/providers
  -> load themes
  -> validate module dependencies and mark modules Resolved
  -> build frontend catalog
       -> compile public frontend roots
       -> compile/import contribution roots
       -> index slots, dependencies, and source fingerprints
  -> instantiate profile roots, or legacy enabled frontend roots
  -> create Tokio runtime + eventfd
  -> start one static file-watcher thread
  -> derive and spawn active backend candidates
  -> start private IPC server
  -> mount frontend components
       -> initialize root/embedded Luau runtimes
       -> apply props/settings/i18n/service context
       -> publish settings and composition snapshots
  -> replay retained service state
  -> publish theme, locale, Started, and optional startup sound
  -> enter shell loop
       1. check theme/settings/frontend reloads
       2. dispatch and route Wayland input
       3. drain/coalesce up to 256 shell messages
       4. tick component runtimes
       5. append deferred effects and transition deadlines
       6. recursively drain every CoreRequest until empty
       7. flush throttled backend commands
       8. build/layout/paint/reconcile child surfaces/present
       9. finish and flush the presentation frame
      10. wait on compositor/eventfd/nearest deadline

Live mutations
  module enable/disable
    -> persist graph/profile decision
    -> reload installed graph
    -> activate/deactivate only frontend runtime kinds
  provider switch
    -> start candidate beside current provider
    -> wait for Running
    -> persist selection
    -> replace active command/runtime slot
  profile switch
    -> load profile + candidate graph + effective settings
    -> build candidate frontend catalog
    -> retain roots by surface/instance id
    -> mount new roots in memory
    -> start changed backend candidates
    -> wait for candidates to report Running
    -> write active-profile
    -> replace catalog, remove old roots, add new roots
    -> replace/stop providers and apply settings/resources
    -> publish graph/catalog/service changes
  package install/uninstall
    -> mutate source, root/profile records, and lock in several steps
    -> rediscover/re-resolve
    -> request live activation/reconciliation

Shutdown
  -> set shutting_down
  -> broadcast ShuttingDown
  -> recursively drain resulting requests
  -> unlink IPC socket
  -> return and let owners/tasks drop
```

The shell boundary should enforce these invariants:

1. Every live object belongs to one immutable activation generation containing
   its graph, interface catalog, settings/resources, frontend catalog, roots,
   providers, watch set, and presentation intent.
2. A candidate cannot publish state, events, commands, surfaces, or persistent
   identity before its generation commits.
3. A root or provider is retained only when its complete runtime identity is
   unchanged, not merely because a string key still exists.
4. All fallible validation, compilation, initialization, readiness, resource,
   and watch preparation happens before the commit point. Failure before commit
   leaves the active generation unchanged.
5. A successful commit reveals ready replacements before retiring old objects.
   Retirement failure is diagnosed but cannot create a mixed active generation.
6. Every message, restart deadline, callback, and command result carries enough
   generation identity to reject work from a candidate, aborted transaction, or
   retired runtime.
7. Effect, message, and callback processing is bounded and fair. One component
   cannot recursively monopolize the shell loop or terminate unrelated modules.
8. Normal shutdown and every error exit run the same ordered, bounded teardown;
   workers cannot outlive the eventfd or other resources they use.

## Severity-ranked findings

### 1. P0 — Profile activation is not one atomic runtime generation

The candidate graph is loaded first, but frontend and backend preparation read
the current mutable `InterfaceRegistry` rather than a candidate interface
snapshot (`profile.rs:457-525`, `567-572`). Existing roots are retained solely
by surface/instance ID (`profile.rs:497-511`), and the profile's `entrypoint`
field is ignored both at startup and during live switching: roots are selected
only by module ID (`discovery.rs:643-667`, `profile.rs:492-528`).

One initial hypothesis was refuted: valid profiles cannot assign the same root
key to a different module because `validate_instance_id` requires the key to
start with `<module-id>#` (`extension/module/src/package/profile.rs:513-528`).
The adjacent defect remains: the same valid root key can change `entrypoint`,
yet the old VM is retained, and even a cold start ignores the requested
entrypoint. The shell must either implement that field or reject every value it
cannot honor.

Commit writes `active-profile` before changing the live runtime
(`profile.rs:728-744`), destroys roots absent from the candidate before adding
prepared roots (`:750-762`), and only later replaces providers, settings,
resources, installed graph, and interface records (`:764-831`). Theme, locale,
and component-setting failures after the pointer write are logged instead of
rolling back (`:789-817`). This contradicts the specified order: prepare hidden
surfaces and services, reveal the new roots, then remove orphans
(`docs/spec/01-module-system.md:774-777`).

Node-slot edits have the same durable/runtime split: the active profile file is
saved before `apply_switch_profile`, and a rejected candidate never restores it
(`profile.rs:144-202`). A restart can therefore select a profile revision the
running shell rejected.

**Improvement:** Introduce an immutable `ActivationPlan`/`RuntimeGeneration`.
It must contain the resolved graph and interface catalog, effective settings
and resources, compiled catalog, full root/provider identities, staged service
state, watch set, and surface plan. Prepare all new runtimes and hidden surfaces
against that snapshot, journal the durable intent, perform one no-fail active
snapshot swap, reveal ready surfaces, and only then retire the old generation.

### 2. P1 — Backend module enable/disable changes persistence but not the live
backend set

`apply_set_module_enabled` persists the composed graph/profile decision and
reloads the installed graph, but its live action branches only for
`ModuleKind::Frontend`; every other kind returns success with no runtime change
(`runtime/request.rs:118-180`). `write_composed_module_enabled` represents
backend enablement through profile `backgroundServices`, so the stored decision
is meaningful (`module_config.rs:205-263`).

**Failure:** Enabling a backend can report success while never spawning it, and
disabling a background service can leave its task and command handler running
until another switch or restart.

**Improvement:** Every graph mutation must produce a typed activation diff:
added/removed/reconfigured providers, roots, interfaces, resources, and
contributions. Apply it through the same activation coordinator and roll back
the persistent decision if preparation fails.

### 3. P1 — Provider messages and restart deadlines are not generation-safe

`BackendRuntimeSlot` identifies a runtime by interface and provider module ID;
the independently spawned event bridge is not retained in the slot
(`backend/spawn.rs:128-143`, `284-299`). Checks for state and method results
compare provider strings, so an old and new generation of the same provider are
indistinguishable (`runtime/mod.rs:349-408`). Profile candidate IDs are
deterministic strings derived from profile/interface/module, so an event queued
by one aborted attempt can match a later attempt with the same names
(`profile.rs:654-668`, `687-725`).

Supervision stores only `restart_pending: bool`. A sleeping task sends
`BackendRestartDue { interface }` with no provider or generation token
(`backend/supervision.rs:18-29`, `110-124`). Provider/profile changes remove the
bookkeeping but do not cancel the task. When it fires, the handler reads the
current graph and unconditionally spawns/replaces a runtime
(`:131-197`; `backend/spawn.rs:88-98`). A stale deadline can therefore replace a
healthy new runtime or resurrect work no longer selected by the active profile.

Named interface events have one additional hole. They are rejected on provider
mismatch only when an active slot exists. With no slot, a queued event from a
stopped or obsolete bridge proceeds to contract validation and component
delivery (`runtime/service_state.rs:410-447`). The broader hypothesis that all
interface events bypass active-provider checks was refuted; the absent-slot
case is the concrete defect.

**Improvement:** Tag backend tasks, bridges, updates, lifecycle messages,
interface events, results, and restart timers with `ActivationGeneration` and a
per-interface `ProviderEpoch`. Retain cancellation/join handles for the service,
bridge, and deadline. Accept a message only when the complete identity is
current; no active slot must mean no provider-owned event is deliverable.

### 4. P1 — Stop, removal, and shutdown bypass authored lifecycle and storage
semantics

`stop_backend_runtime` removes routing and calls `AbortHandle::abort()`
(`backend/lifecycle.rs:63-85`). Backend `stop(self)` and storage flush run only
when the backend loop exits normally (`runtime/backend/src/lib.rs:361-373`), so
provider switches, profile retirement, and candidate aborts can skip them. The
event bridge task is detached and survives independently.

The frontend host trait has `mount` but no `unmount` lifecycle hook
(`frontend/host/src/lib.rs:471-501`). Profile removal destroys surfaces and
drops the component directly (`profile.rs:839-853`); module deactivation follows
the same shape (`discovery.rs:792-844`). The script context's Rust `Drop` can
flush storage, but it cannot run authored unmount behavior.

Normal shell shutdown broadcasts one event, drains its effects, removes the
socket path, and returns (`runtime/mod.rs:320-327`). It does not reject new work,
cancel restart deadlines, gracefully close and await providers, unmount
components, destroy child/parent surfaces, stop IPC, or stop/join the watcher.
Any `?` error after workers start bypasses even the success-only socket removal
(`runtime/mod.rs:231-325`).

**Improvement:** Make frontend and backend teardown explicit and idempotent.
Gracefully close command channels, invoke `unmount`/`stop`, flush storage, await
with deadlines, and use abort only as a diagnosed last resort. All success and
error exits must enter one shell shutdown state machine.

### 5. P1 — Detached workers can outlive the raw eventfd they write

The shell owns the eventfd, but file-watch, IPC, backend-bridge, and restart
workers receive only a raw integer descriptor (`file_watch.rs:7-19`,
`ipc.rs:12-17`, `backend/supervision.rs:31-39`). The watcher thread discards its
join handle, and IPC accept/client tasks are detached. Each later reconstructs
a borrowed fd around that raw number before writing. If `run()` returns through
an error and drops/reuses the owned fd while a worker remains alive, a worker
can write through a stale or reused descriptor.

**Improvement:** Workers must own a cloned safe wake handle or send through an
owner that remains alive until they join. A lifecycle guard should own
cancellation tokens, task/thread handles, IPC socket cleanup, presentation
objects, and the eventfd; drop the eventfd last.

### 6. P1 — Component and reload errors can terminate the whole shell instead
of isolating the failing module

The main loop propagates errors from message delivery, ticks, requests,
rendering, and presentation with `?` (`runtime/mod.rs:248-284`). Service delivery
maps one component callback failure directly to `ShellRunError`
(`runtime/service_state.rs:239-347`). Frontend and theme reload parse/compile
errors also escape `Shell::run` (`runtime/reload.rs:27-40`,
`runtime/theme.rs:113-140`).

This conflicts with the lifecycle contract: a broken module should transition
to `Errored`, show a bounded placeholder, and leave unrelated modules running
(`docs/spec/01-module-system.md:886-905`). It also turns a transient half-written
development source or CSS file into whole-shell termination instead of
last-known-good reload behavior.

**Improvement:** Put every component/runtime behind a supervisor and contain
callback, tick, render-build, and reload failures at that identity. Discard the
failed effect batch, preserve the last-known-good compiled/runtime snapshot or
install a bounded error placeholder, record an actionable diagnostic, and use a
bounded restart/quarantine policy. Presentation connection loss remains a
shell-level failure but must still run full cleanup/recovery.

### 7. P1 — Watch coverage is a startup snapshot, while fallback polling is
parked for 24 hours

The file watcher is spawned once from the current theme, settings, and mounted
component source paths (`runtime/mod.rs:215-216`, `443-450`). It converts those
paths to a fixed directory list and never accepts updates (`file_watch.rs:7-24`,
`36-53`, `82-110`). Frontend reload can replace `runtime.source_paths` after an
import changes, and settings/profile changes can replace `theme_watch`, but
neither rebinds inotify (`runtime/reload.rs:42-55`,
`runtime/theme.rs:372-384`). Graph/profile/module/lock files, contribution-only
sources, and graph i18n catalogs are also absent from this manager.

When `file_watcher_active` is true, metadata polling is parked for 24 hours
(`runtime/mod.rs:17`; `runtime/reload.rs:13-18`; `runtime/theme.rs:103-111`,
`316-323`). A watcher that exits on an inotify error cannot clear that bool, so
new or changed paths may remain stale for a day.

**Improvement:** Replace the detached one-shot watcher with a managed,
generation-aware `WatchSet`. Reconcile it after every graph, catalog, profile,
theme, locale, and import change. Watch contribution/resource/profile inputs,
report watcher health, and immediately restore bounded polling if the watcher
dies. Catalog/resource reload remains last-known-good and commits atomically.

### 8. P1 — Transitive CoreRequest processing is unbounded and re-entrant across
frame phases

`drain_requests` repeatedly applies a request and appends all emitted requests
until the queue is empty, with no count, time, size, provenance, or cycle limit
(`runtime/request.rs:560-568`). It is called from the main loop and also during
input and render/close paths (`runtime/wayland.rs:87-89`, `153-154`, `349-352`;
`runtime/render/mod.rs:25-33`). Effects can therefore mutate topology in the
middle of input/render processing.

A concrete cycle is possible when a component reacts to the `mesh.theme`
snapshot by requesting `SetTheme`: `apply_set_theme` always republishes theme
state (`runtime/theme.rs:161-194`, `246-308`), which can produce the same request
again. The shell can hang before presentation and grow the queue indefinitely.

**Improvement:** Use one bounded effect scheduler at a defined frame phase.
Tag effects with source module/runtime/generation and causal transaction ID,
apply per-frame/per-module count and byte budgets, detect repeated causal
cycles, defer fair residual work, and quarantine a producer that repeatedly
exceeds policy. Topology changes should commit between frame snapshots, not
inside arbitrary input or render callbacks.

### 9. P1 — Provider failure changes only the cache; observers keep stale
healthy state

Terminal backend handling removes the runtime and calls
`clear_active_provider_service_state` (`backend/lifecycle.rs:218-232`). That
method replaces `latest_service_state` with an `available: false` payload but
never passes it to `deliver_service_event` (`:236-265`). The existing regression
checks only the cache (`shell/tests/backend_lifecycle.rs:867-901`).

Prepared providers have the inverse problem: candidate state updates are tagged
with a synthetic provider ID and ignored before commit. Readiness is based on
`Running`, not a buffered validated initial snapshot, so initial state can be
lost until a later poll. This confirms the Section 3/11 provider-readiness
finding at the shell integration seam.

**Improvement:** A provider generation becomes ready only after initialization
and a validated initial state (where the contract requires one). Buffer its
state/events during preparation, publish them with the provider epoch at
commit, and publish one typed unavailable transition immediately on retirement
or failure. Recovery must replace that transition with new current state.

### 10. P2 — Control-plane state and follow-up effects can diverge

Theme and locale writes are live-only in this package, while icon/font writes
use the persistence helper (`runtime/theme.rs:161-243`, `455-471`). Their durable
ownership is already recorded by the Section 4/5 audits and should be fixed by
the shared settings/profile transaction rather than a shell-only patch.

There are two additional shell propagation gaps:

- `apply_set_module_prop` persists and applies the effective store but does not
  republish `mesh.settings`, so observers can retain its old revision
  (`profile.rs:321-420`).
- theme/locale change helpers call `broadcast_core_event` and discard every
  returned `CoreRequest` (`runtime/theme.rs:143-158`, `441-452`).

The shell also sends an internal, undeclared `"set-current"` command directly
to a `mesh.theme` backend (`runtime/theme.rs:246-308`), despite the built-in
contract declaring only public setters (`discovery.rs:262-278`).

**Improvement:** Route all control-plane writes through declared, revisioned,
durable operations. A successful transaction publishes settings/theme/locale
snapshots and schedules component effects in one ordered batch. Host-derived
provider state needs an explicit typed synchronization contract, not a magic
method string.

### 11. P2 — Invalid graph/profile startup degrades asymmetrically and can hide
the configured composition

If the installed graph fails to load, frontend catalog construction drops the
error and treats the graph filter as absent; `installed_enabled_frontend_ids`
then returns `None`, meaning all discovered frontend modules may mount
(`discovery.rs:631-640`, `877-895`). Backend startup handles the same error by
starting no services (`backend/spawn.rs:13-57`). A malformed graph can therefore
produce an unintended UI with no providers instead of preserving or rejecting
one coherent composition.

With an active profile, a root whose module has no mountable catalog entry is
silently skipped during cold startup (`discovery.rs:643-669`), whereas live
profile switching rejects it (`profile.rs:509-519`). An active-profile load
error is also treated as absence and falls back to legacy composition
(`discovery.rs:208-225`).

**Improvement:** Distinguish intentional legacy/no-profile operation from an
invalid configured graph/profile. Cold and live activation must use the same
validator and diagnostics. Invalid candidates leave a last-known-good snapshot
active; when none exists, start an explicit recovery shell rather than enabling
unselected modules.

### 12. P2 — Package mutations confirm the Section 2 transaction gap at the
running-shell seam

Install copies into the final module destination before later graph, lock, and
activation steps; a lock or activation failure does not restore every earlier
mutation (`package.rs:18-117`, `488-625`). Uninstall edits profiles and root
state, stops runtimes, deletes source, rebuilds the graph/catalog, and only then
archives/saves the lock (`package.rs:119-306`). Failure injection at a later
step can leave disk and live state disagreeing.

This is not a second backlog item: Section 2 already owns the required locked,
journaled transaction engine. Section 15 should consume that engine's prepared
candidate and make its activation generation the runtime half of the same
transaction.

## Unconstrained feature direction

The better feature is a revisioned shell activation coordinator, not more
special-case rollback inside `profile.rs`, `package.rs`, and `request.rs`.

```text
Stable(g)
  -> Preparing(txn, candidate g+1)
       immutable graph + interfaces + resources + settings
       full root/provider identity diff
       compiled roots/contributions
       hidden frontend surfaces
       unpublished backend tasks + buffered initial state
       candidate watch set
  -> Ready(txn)
       every required runtime initialized
       required initial service state validated
       first surface configure/paint readiness established
       all fallible durable writes staged
  -> Committing(txn)
       durable journal records candidate generation
       atomic in-memory ActiveSnapshot swap
       publish provider epochs/catalog/service state
       reveal replacement surfaces as one presentation batch
       active-profile/transaction marker commits durably
  -> Active(g+1)
  -> Retiring(g)
       unmount roots, stop providers, flush storage
       destroy children then parents, clear input/popover indexes
       cancel/join old bridges/restart timers/watch generation
  -> Stable(g+1)

Any failure before Committing
  -> cancel/join candidate workers
  -> destroy hidden candidate surfaces
  -> discard candidate storage overlay/watch set/staged writes
  -> remain byte-for-byte Stable(g)
```

An `ActiveSnapshot` should own at least:

```text
ActivationGeneration
profile id + revision/hash
InstalledModuleGraph
immutable InterfaceCatalog
FrontendCatalog revision
effective SettingsStore + theme/locale/resource generations
roots: RootInstanceKey -> RootRuntimeIdentity + handles
providers: Interface -> ProviderEpoch + identity + service/bridge handles
watch set + watcher health/generation
presentation surface generation map
```

Root retention compares `(instance key, module id, entrypoint, compiled content
identity, granted capabilities, relevant settings ABI)`. Provider retention
compares `(interface, module, contract/version, entrypoint/content digest,
capabilities, effective settings)`. Stable storage identity can survive a
runtime replacement without preserving the old VM.

The shell loop then becomes a small coordinator over immutable snapshots:
collect bounded inputs, produce a typed effect batch, validate/apply one state
transaction, build one render snapshot, present, and retire old generations in
the background. This also enables a useful feature beyond current flow:
preview a candidate profile in isolated hidden surfaces, report its health and
diagnostics to settings/developer tools, and let the user commit only after the
candidate is visibly and service-ready.

Shutdown is another transition of the same supervisor:

```text
Running -> Quiescing -> StoppingComponents -> StoppingProviders
        -> DestroyingPresentation -> StoppingWorkers -> Flushing -> Stopped
```

Each phase has a deadline and diagnostics. New external work is rejected after
`Quiescing`; already accepted bounded effects settle or fail explicitly.

## Recommended implementation order

1. Add generation/epoch identity to backend slots, bridges, all backend shell
   messages, restart deadlines, and pending profile/provider transactions.
   Reject absent-slot and stale-generation events.
2. Introduce `ActiveSnapshot` plus a pure activation diff. Correct root
   entrypoint/identity handling and backend enable/disable reconciliation before
   changing persistence order.
3. Stage candidate interfaces, providers with buffered readiness state, roots,
   resources, and hidden surfaces; commit replacements before retirement.
4. Connect the Section 2 transaction journal to profile/package activation so
   the active pointer is a recoverable commit marker rather than the first live
   mutation.
5. Add explicit frontend unmount and graceful backend stop/join, then route
   normal and error exits through the lifecycle supervisor.
6. Replace the static watcher with a managed generation-aware watch set and
   last-known-good reload coordinator.
7. Centralize bounded effect scheduling and per-component failure isolation.
8. Unify control-plane snapshot publication and add the complete regression
   matrix before performance tuning this orchestration layer.

## Regression matrix

| Area | Regression |
| --- | --- |
| Profile validation | A root key for another module is rejected; this refuted scenario never reaches runtime diffing |
| Entrypoint | Cold start and live switch honor a non-default supported entrypoint, or reject it before activation |
| Root identity | Changed entrypoint/content/capability identity replaces the VM while preserving only declared durable storage |
| Candidate snapshot | Frontend/backend preparation resolves only contracts and providers from the candidate graph generation |
| Failed root | Compile, mount, configure, or first-ready failure leaves every old root visible and active |
| Commit order | Replacement is ready/revealed before its orphan is retired; no blackout window |
| Active pointer | Failure before commit preserves the old pointer; crash recovery resolves a journaled commit deterministically |
| Node slot | Failed live activation restores/stages the prior profile file and keeps runtime/disk generations aligned |
| Backend toggle | Enabling starts the selected required backend; disabling stops it and publishes unavailability |
| Provider retention | Same module with changed contract/settings/content receives a new epoch and runtime |
| Initial state | Candidate state is buffered and published exactly once with readiness/commit |
| Failure state | Mounted observers receive `available=false` immediately; recovery replaces it with current state |
| Stale update | Old same-provider generation cannot publish state, events, lifecycle, or method results |
| No active slot | Queued provider interface event after stop reaches no component |
| Restart timer | Delayed pre-switch/pre-disable/pre-shutdown deadline is ignored and cannot replace a healthy runtime |
| Stop | Provider `stop`, stream cleanup, and storage flush run before bounded abort fallback |
| Unmount | Removed/replaced frontend runs authored unmount and clears every surface/input/service index |
| Component failure | One callback/tick/build failure yields a bounded placeholder; other roots continue |
| Reload failure | Invalid partial `.mesh` or theme edit preserves the last-known-good runtime and reports diagnostics |
| Watch import | Reload adding a source in a new directory rebinds the watch set; its next edit reloads promptly |
| Watch resources | Profile, graph, manifest, contribution, theme, locale, and i18n inputs update the correct generation |
| Watcher death | Inotify failure flips watcher health and restores short bounded polling, never a 24-hour blind spot |
| Effect cycle | Self-emitting theme/request loop hits a per-source causal budget without blocking input/presentation |
| Effect fairness | A large legitimate batch is continued across frames without reordering its transaction |
| Settings state | Prop/theme/locale changes publish one matching revision and preserve returned component effects |
| Invalid graph | Cold and live paths reject the same candidate; no all-frontends/no-backends fallback split |
| Shutdown | Pending candidates, timers, IPC clients, watcher, components, providers, and surfaces all terminate in order |
| Error exit | Any injected post-start error executes the same cleanup and keeps eventfd alive until workers join |
| Package failure | Section 2 journal rolls source/root/profile/lock/runtime activation forward or back as one recoverable transaction |

## Verification

- `nix develop -c cargo test -p mesh-core-shell --lib`: 666 passed, 7 failed,
  130 ignored. The failures are the current dirty-tree baseline and are unrelated
  to this Markdown-only audit.
- Source/spec inspection confirmed the runtime loop order, profile commit order,
  frontend-only live enable/disable branch, provider timer/message identities,
  unavailable-state delivery omission, static watch set, unbounded request
  drain, and success-only shutdown cleanup.
- No performance claim is made and no production code was changed.
