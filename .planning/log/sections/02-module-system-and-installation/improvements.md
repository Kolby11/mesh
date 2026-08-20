# Section 2 — Module system and installation: improvement audit

**Audited:** 2026-08-17  
**Scope:** `mesh-core-module`, including canonical and legacy manifest loading,
installed graphs, resolution, profiles/compositions, contributions, locks,
lifecycle, health, and the shell/CLI package consumers.

This is a point-in-time review record, not a second task tracker. Open work from
this audit lives in [`docs/BACKLOG.md`](../../../../docs/BACKLOG.md).

## Logical process map

```text
module source / Git revision
        │
        ▼
canonical module.json loader
        │ parse → normalize → typed validation
        ▼
staged module inventory + provenance
        │
        ▼
candidate installed graph
        ├─ required/optional dependency closure
        ├─ interface contracts + compatible providers
        ├─ profile/composition/resources closure
        ├─ contribution/source index
        ├─ capability decisions
        └─ static diagnostics + health
        │
        ▼
transaction preparation
        ├─ compile frontend candidates
        ├─ prepare backend candidates
        ├─ calculate root/lock/profile changes
        └─ explain capability/graph diff
        │
        ▼
atomic persistent commit ──► runtime activation commit
        │                            │
        ├─ source store              ├─ frontend mounts
        ├─ root inventory            ├─ backend lifecycles
        ├─ lock/provenance           └─ provider/service state
        └─ profiles/active pointer           │
                                             ▼
                                  runtime health + recovery
```

The current implementation has two partly independent manifest paths, two
package authorities, and several runtime state stores:

```text
generic legacy-capable loader → shell discovery → catalog/mount
canonical loader              → installed graph → enabled/health view

CLI package code   ─┐
                    ├─ mutate source/root/lock/profile in different orders
shell package code ─┘

ModuleInstance lifecycle ≠ frontend state ≠ backend state ≠ graph health
```

The missing candidate-graph and transaction boundaries explain most of the
findings below.

## Confirmed findings

### 1. Critical — module-controlled paths escape the package boundary

`ModuleManifest::validate` only rejects an empty module name
(`crates/core/extension/module/src/package/module_manifest.rs:51`). Shell and
CLI installers then derive destinations with
`modules_dir.join(name.trim_start_matches('@'))`
(`crates/core/shell/src/shell/package.rs:42` and
`crates/tools/cli/src/main.rs:544`). A name such as `@scope/../../outside`
therefore writes outside `modulesDir` before the shell performs its late lexical
`strip_prefix` check. The CLI performs no containment check. CLI uninstall uses
the caller-supplied ID in the same pattern and passes it to `remove_dir_all`
(`tools/cli/src/main.rs:1045`), making traversal an arbitrary-directory deletion
path.

Backend `entrypoints.main` is not validated as module-relative. Candidate
construction joins and reads it directly
(`crates/core/shell/src/shell/backend/candidates.rs:112`), after which the Luau
runtime executes it. A path such as `../../outside/payload.luau` escapes both
module provenance and the package-content boundary.

Local-copy installs reject symlinks, but Git installs rename the checkout into
place without the same validation (`crates/core/shell/src/shell/package.rs:488`).
Git symlinks can redirect manifests, contracts, entrypoints, components, and
assets outside the module. Recursive discovery/static scanning uses
`Path::is_dir` with no symlink rejection or visited set
(`crates/core/extension/module/src/package/installed_graph/scan.rs:373`), so a
directory symlink can scan outside the module or recurse until path limits.
Module digests silently omit symlinks, so the executed/read tree can differ from
the locked content.

Component local-import classification also accepts absolute and `../` targets,
and the compiler reads them without proving containment under the module root
(`crates/core/ui/component/src/parser/script.rs:134` and
`crates/core/frontend/compiler/src/compile.rs:207`). Most declared contribution
paths already have sound lexical validation; plain external-contract `../`
traversal is therefore **not** a defect. Symlink redirection remains a defect for
those paths.

**Improve it:** introduce shared typed `ModuleId`, `ModuleRelativePath`, and
`InstallDestination` constructors. Validate before every read/write/delete,
canonicalize existing components, prove containment, reject installed-content
symlinks, and make scanners use `symlink_metadata` plus a visited set. Apply the
same validation to manifests, roots, locks, CLI arguments, entrypoints, imports,
contracts, and assets.

### 2. Critical — graph failure fails open and can activate rejected or disabled modules

When installed-graph loading fails, shell discovery returns `None` and explicitly
falls back to every discovered frontend
(`crates/core/shell/src/shell/discovery.rs:877`). The frontend catalog likewise
compiles all discovered modules when its enabled-ID filter is absent
(`crates/core/shell/src/shell/component/catalog.rs:419`). The shell can therefore
mount modules that the graph rejected or marked disabled. Duplicate discovery IDs
can overwrite one another even though graph construction rejects them.

Provider/interface registration has a related split: shell discovery registers
providers from every discovered module before graph filtering, then appends graph
providers. A disabled high-priority provider can consequently win contract or
state validation even though an enabled provider is the one that launches
(`crates/core/shell/src/shell/discovery.rs:510` and
`shell/backend/candidates.rs:253`).

**Improve it:** represent graph loading as an explicit valid/invalid candidate,
never as `Option<filter>`. Build fresh catalogs and registries exclusively from
the enabled compatible graph. On startup failure activate no unapproved modules;
on reload failure retain the last known-good graph and runtime.

### 3. Critical — install, update, uninstall, and rollback are not one transaction

CLI and shell independently implement package mutation, contrary to the spec’s
package-service model. Their behavior already differs:

- CLI install never updates a non-empty explicit root inventory, so the graph
  cannot discover the module; the shell does update it.
- CLI uninstall removes only source and lock state, leaving root/profile/provider/
  resource/runtime references; the shell attempts broader cleanup.
- CLI update checks out repositories in place one at a time, then saves the
  lock. If candidate two or the lock write fails, candidate one remains updated
  while the lock still records its old revision
  (`crates/tools/cli/src/update.rs:302`).
- Rollback restores only modules present in the target lock, skips missing
  directories, leaves extra current modules installed, ignores root/profiles/
  runtime, and writes the target lock outside the normal archive/generation path
  (`tools/cli/src/update.rs:348`).
- Shell install rolls back early graph/kind failures, but a later lock,
  resolution, profile, or activation failure leaves prior mutations committed
  (`crates/core/shell/src/shell/package.rs:74`). Local copy failure can also
  leave a partial destination.
- Shell uninstall saves profiles, stops runtimes, changes root/source/catalog,
  and only then commits the lock; a late failure leaves a mixed state.

Neither authority takes an inter-process transaction lock, so simultaneous CLI
and shell writes can lose root, lock, or profile updates. Staging directories
live inside the discoverable module tree; a crash can leave a partial Git clone
that discovery treats as installed.

**Improve it:** put one transaction engine in `mesh-core-module::package`; CLI
and shell become clients. It should acquire an OS lock, recover an incomplete
journal, stage outside the live module tree, validate the complete candidate
graph, prepare the runtime without visible mutation, atomically swap source/root/
lock/profile state with backups, commit the runtime, then mark the journal
complete. Install, update, uninstall, and rollback must use the same engine.

### 4. High — dependency and provider compatibility is computed but not enforced

The resolver computes missing required modules and version conflicts, but graph
construction converts only conflicts to diagnostics and never consumes
`resolution.missing` as an activation gate
(`crates/core/extension/module/src/package/installed_graph/graph.rs:399`). Even
version conflicts remain diagnostic-only, so an incompatible component may
still compile and mount.

Optional module metadata exists, but profile closure queues every dependency as
required (`crates/core/extension/module/src/package/profile.rs:292`). Malformed
version ranges are treated as satisfiable; a current unit test codifies that
behavior (`package/resolution.rs:294`). Interface requirements retain version
ranges, yet provider selection and backend contract validation resolve by name
with no compatible-range check (`crates/core/shell/src/shell/backend/candidates.rs:159`).

**Improve it:** parse module and interface ranges during manifest validation.
Candidate resolution must block only affected modules for missing/conflicting
required edges, retain optional edges as explicit degraded records, and select
providers only when their contract version satisfies every active consumer.

### 5. High — composition/profile resolution drops declared intent

`CompositionRef.version` is deserialized but ignored during resolution
(`crates/core/extension/module/src/package/composition.rs:137`). Dependencies,
resources, interfaces, and version requirements declared by a composition or
its `extends` chain are not carried into `ShellProfile::active_module_ids`, so a
profile can claim a pin or dependency it never checks.

The spec’s sparse inherited-root override `{ "active": false }` cannot
deserialize because `ProfileRootInstance.module` is mandatory
(`package/profile.rs:43`). Orphaned node-slot overrides are deliberately retained
and then activate their contributed modules even when the host root is absent or
inactive. `LockedComposition` exists, but normal installation does not populate
lock composition state.

**Improve it:** resolve composition layers into a typed delta model first, then
require concrete root modules after merging. Verify the selected composition
version, include every composition edge in the closure, and derive active node
slots only from active hosts. Persist resolved composition provenance in the
lock.

### 6. High — update planning does not validate the candidate graph or external contracts

Update planning validates candidate manifests individually but does not build a
candidate dependency/provider/profile closure before checkout
(`crates/tools/cli/src/update.rs:157`). External contract compatibility reads
only `module.json`; string contract references are not resolved like installed
graph loading does. A breaking change in `contract.json` can therefore evade the
compatibility diff.

Capability review has the Section 1 flaw as well: optional capability additions
are omitted and no durable approval decision exists. That cross-section issue is
tracked in the Section 1 audit rather than duplicated here.

**Improve it:** make update a graph-to-graph plan over fully materialized staged
sources. Resolve external contracts, compare public contracts and capability
decisions, explain dependency/provider/profile changes, and refuse before any
checkout or persistent mutation.

### 7. High — canonical and legacy manifest policies disagree

The installed-graph loader accepts only canonical `module.json` and produces
migration diagnostics for `package.json`, `mesh.toml`, or legacy shapes
(`crates/core/extension/module/src/package/installed_graph/load.rs:180`). The
generic loader still converts those inputs into runnable manifests, and shell
discovery calls it (`crates/core/extension/module/src/manifest/load.rs:29` and
`crates/core/shell/src/shell/discovery.rs:164`). Combined with graph fail-open,
legacy or incompletely validated modules can reach the catalog through the path
that the canonical graph rejects.

Direct `mesh.capabilities.required/optional` strings also skip the validation
applied to equivalent `mesh.uses.capabilities` declarations
(`package/module_manifest.rs:330` and `:776`).

**Improve it:** make one canonical normalized loader the only production API.
Move legacy readers behind an explicit migration command, normalize equivalent
fields into typed values, validate once, and use the same result for graph,
shell, CLI, and tools.

### 8. Medium-high — lifecycle and health models are not runtime authority

`ModuleInstance` defines discovered, resolved, loaded, initialized, running,
suspended, unloaded, and errored states, but workspace consumers only perform
`Discovered → Resolved` (`crates/core/extension/module/src/lifecycle.rs:6` and
`crates/core/shell/src/shell/discovery.rs:606`). Frontend/backend runtime state,
errors, restart/suspension, and quarantine live elsewhere. `should_disable`,
error counters, and timestamps consequently do not describe the running module.

Graph health is chiefly static binary/provider health and does not reduce
dependency failures or runtime failures into one authoritative state. When a
backend fails, cached service state is replaced with `available: false`, but the
transition is not delivered to existing consumers, which can retain stale
healthy state (`crates/core/shell/src/shell/backend/lifecycle.rs:218`).

**Improve it:** keep separate static graph health and runtime health streams,
then derive module/provider aggregates. A lifecycle coordinator should own
runtime transitions, recovery, unload, restart, and quarantine, and broadcast
service availability changes to current consumers.

### 9. Medium — lock and graph metadata are incomplete or nondeterministic

Code enforces lock schema version 2 while the installation spec’s example uses
version 3 (`crates/core/extension/module/src/package/lock.rs:25` and
`docs/spec/02-installation.md:150`). Choose one authoritative contract and add
explicit migrations.

Both installers write an empty `requested_by` set even though uninstall uses it
to prevent removal of dependencies. Installing A → B can therefore leave B
apparently removable. Composition lock metadata is likewise not populated.

Duplicate standalone interface declarations and extension-point declarations
overwrite `HashMap` entries without deterministic diagnostics. Duplicate
same-module contribution IDs survive manifest validation but collide in the
compiled catalog key. These should all be rejected before candidate activation.

## Architectural improvements beyond the current flow

1. Use a content-addressed, immutable module store and activate generation
   snapshots rather than mutating live Git checkouts.
2. Make `ResolvedModuleGraph` the sole inventory and runtime input, containing
   normalized manifests, provenance/digests, enabled closure, compatible
   providers, contributions, health, and effective capability decisions.
3. Provide graph-diff/dry-run output explaining added grants, provider changes,
   disabled modules, contract breaks, and profile effects before commit.
4. Separate manifest graph construction from cached incremental `.mesh`/Luau
   indexing. Source parse failures should be scoped diagnostics, not silent
   disappearance or whole-graph ambiguity.
5. Integrate signed provenance/trust tiers with the same candidate-plan policy,
   rather than adding a second installer gate later.

## Recommended implementation order

1. Land `ModuleId` and contained module-relative path primitives; apply them to
   all read/write/delete paths and reject Git symlinks.
2. Make canonical loading and installed-graph authorization fail closed.
3. Correct dependency/interface/provider/composition resolution and validate all
   typed ranges before activation.
4. Build the shared candidate-graph API, including source/contract and capability
   review.
5. Move all package mutation behind the locked journaled transaction engine and
   make CLI a package-service client.
6. Wire authoritative lifecycle and static/runtime health propagation.
7. Migrate lock/provenance metadata and reject duplicate contract/contribution
   identities.
8. Add immutable generations, incremental source indexes, dry-run explanations,
   and trust/signing policy.

## Required regression coverage

- Traversal module names/CLI IDs and escaping entrypoints/imports are rejected
  before any filesystem operation.
- Git symlinked files/directories are rejected; recursive scanners terminate and
  never return content outside the module root.
- Invalid graph startup activates no unapproved module; failed live reload keeps
  the prior graph/runtime.
- CLI and shell operations produce identical state; injected failure at every
  transaction phase restores source, root, lock, profiles, and runtime.
- Concurrent package operations serialize without lost updates; crash recovery
  completes or restores an interrupted journal.
- Required missing/conflicting dependencies and incompatible interface providers
  block affected activation; missing optional modules only degrade.
- Composition versions, dependencies, sparse root deactivation, and active-host
  node slots resolve as declared.
- Candidate updates include external contracts and full closure validation before
  checkout.
- Legacy manifests are rejected consistently by discovery, graph, CLI, and
  runtime.
- Backend failure broadcasts service unavailability and updates authoritative
  lifecycle/health.
- Lock requester/composition metadata prevents unsafe removal and reproduces the
  selected generation.
- Duplicate interface, extension-point, and same-module contribution IDs yield
  deterministic diagnostics.

## Verification

Six Luna xhigh passes reconstructed the end-to-end process, independently
reviewed logic and concrete defects, and performed focused filesystem,
transaction, and resolver/profile audits. The filesystem and resolver passes
explicitly refuted or narrowed earlier claims where existing validation applied.

Executed locally under `nix develop`:

```text
mesh-core-module: 198 passed, 0 failed, 3 ignored
```

The ignored tests are release-only loading/scan benchmarks. No production code
was changed by this audit.
