# Section 02 — Module system and installation

## Scope and coverage

Reviewed all 72 assigned files: `mesh-core-module` manifests, graph,
resolution, lifecycle, lock/content-store, package transaction and tests;
legacy fixture manifests under `config/modules/`; the shipped non-interface
`module.json` files; and the module/installation specifications. Repository-
wide callers in shell, CLI, LSP, runtime, frontend, service, and resource
packages were searched. **72/72 assigned files inspected; no follow-up remains.**

## Process tree

```text
module.json / root graph / profile / installed source
  -> canonical manifest discovery and migration diagnostics
  -> dependency/provider/resource closure
  -> capability/trust/health validation
  -> candidate graph and contribution indexes
  -> package transaction: journal, backups, staged source, lock/store snapshot
  -> durable graph/profile/active pointer commit
  -> shell activation and lifecycle reconciliation
  -> rollback/recovery or cleanup
```

The transaction and graph objects provide useful candidate boundaries, but some
paths validate an old active store before publishing a candidate, and profile
delta deserialization loses whether fields were omitted or explicitly empty.
The durable package lock protects transaction concurrency, while profile and
settings revisions still have separate higher-level races. The authoritative
spec also contains a missing-required-provider policy contradiction that must be
resolved before a single activation behavior can be tested.

## Performance findings

### S02-PERF-001 — Blocking package/Git work runs on the shell request path

- **Source:** `crates/core/shell/src/shell/runtime/request.rs:495-503,1532`,
  `crates/core/extension/module/src/package/transaction.rs:510-570`.
- **Current behavior:** package requests are processed synchronously; Git
  clone, checkout, and `rev-parse` are blocking subprocess operations on that
  path.
- **Why it matters:** install/update/uninstall can stall input and frame
  processing, especially on network or large repositories.
- **Improvement:** stage and validate in a bounded worker, returning a typed
  preparation result; keep only the serialized commit/acknowledgement boundary
  on the shell thread.
- **Measurement:** release `nix develop` runs with local trees and Git clones
  of known sizes while generating 60 Hz input; compare max frame gap, p95
  request latency, wall time, and cancellation behavior.
- **Confidence:** confirmed blocking path, unmeasured user impact. **Status:**
  new; adjacent to the existing blocking-I/O backlog.

### S02-PERF-002 — Every package operation eagerly copies and fsyncs broad state

- **Source:** `transaction.rs:622-641,988-1040`, CLI update path at
  `crates/tools/cli/src/main.rs:1295`.
- **Current behavior:** `protect_package_state` snapshots modules, profiles,
  lock history, pointers, and store state before mutation; copied files are
  individually synchronized, including dry runs/no-op planning.
- **Why it matters:** I/O cost scales with the whole package state instead of
  the mutation, making dry runs and small updates expensive.
- **Improvement:** preserve crash recovery while evaluating lazy per-target
  backups, reflink/immutable-store snapshots, and a transaction plan that
  avoids writes for dry-run.
- **Measurement:** no-op, dry-run, and update workloads over module trees of
  known byte size; count bytes copied, fsyncs, syscalls, p50/p95 duration, and
  recovery behavior before/after.
- **Confidence:** confirmed behavior, impact hypothesis. **Status:** new.

### S02-PERF-003 — Authoring refresh rehashes every module tree

- **Source:** `package/authoring.rs:39-74` and LSP refresh in
  `crates/tools/lsp/src/backend.rs:270-293`.
- **Current behavior:** an authoring snapshot computes `module_tree_digest` for
  every graph module; LSP refresh invokes that whole-catalog path on refresh.
- **Why it matters:** a one-file editor change rereads all installed module
  trees, increasing editor latency and filesystem load.
- **Improvement:** use reliable per-module/file watcher generations or a
  content-addressed index, retaining full hashing when a watcher event is
  missing or trust changes.
- **Measurement:** repeated LSP saves over the 29 shipped modules plus a
  128-module synthetic catalog; measure bytes read, directory calls, p50/p95
  refresh latency, and stale-index rate.
- **Confidence:** confirmed behavior, unmeasured impact. **Status:** new;
  related to Section 16’s canonical authoring snapshot work.

### S02-PERF-004 — Manifest loading and tree validation perform redundant passes

- **Source:** `installed_graph/load.rs:380-404`,
  `package/module_manifest.rs:35-48`, `lock.rs:552-557`.
- **Current behavior:** canonical load reads and structurally parses manifest
  content before `ModuleManifest::from_path` rereads/reparses it; tree digest
  validation and file collection are separate traversals.
- **Why it matters:** cold discovery and transaction planning pay repeated
  reads, JSON parsing, and directory walks across all modules.
- **Improvement:** share an in-memory parsed manifest and combine compatible
  validation/index passes while preserving migration and source-location
  diagnostics.
- **Measurement:** cold graph load/install over shipped and large synthetic
  trees; count JSON parses, bytes read, directory entries, allocations, and
  CPU time across repeated release runs.
- **Confidence:** confirmed behavior, unmeasured impact. **Status:** new.

### S02-PERF-005 — Recursive source scanning has no explicit size budget

- **Source:** `package/installed_graph/scan.rs:317-370`.
- **Current behavior:** every matching `.mesh` source is read fully into a
  `String` during recursive scanning.
- **Why it matters:** a malformed or hostile module can force high peak memory
  and graph-build latency through many or very large files.
- **Improvement:** define per-file, per-module, and aggregate source limits;
  reject over-budget sources with bounded diagnostics while retaining the
  failed path in the watch set.
- **Measurement:** oversized and high-file-count fixtures; measure peak RSS,
  bytes read, diagnostic size, and build time. Do not set limits without
  representative authoring workloads.
- **Confidence:** confirmed unbounded behavior; severity speculative. **Status:**
  new.

## Dead code and redundancy

### S02-DEAD-001 — Legacy DFS and migration readers are not production authorities

- **Source:** `manifest/graph.rs:10` and re-exports/tests; `manifest/json.rs`,
  `manifest/toml.rs`, and `manifest.rs:6`; current resolution at
  `package/resolution.rs:136`.
- **Current behavior:** `validate_module_dependency_graph` is only used by
  its definition/re-export and migration tests; canonical resolution uses
  `resolve_closure`. JSON/TOML migration readers are explicitly test-only.
- **Why it matters:** retaining public legacy paths encourages alternate
  manifest/graph behavior and weakens the one canonical `module.json` model.
- **Improvement:** privatize or remove the unused DFS API after downstream API
  review, and keep migration parsers isolated as test/diagnostic fixtures.
- **Test:** full workspace compile, graph closure tests, and legacy-manifest
  rejection diagnostics.
- **Confidence:** confirmed repository-unconsumed, with external API risk.
  **Status:** possible dead code; related to older audit, not a current loader
  bug.

### S02-DEAD-002 — Install capability review is duplicated in shell and CLI

- **Source:** `crates/core/shell/src/shell/package.rs:686` and
  `crates/tools/cli/src/main.rs:1013`.
- **Current behavior:** both frontends implement `check_install_capabilities`
  rather than consuming one typed core review result.
- **Why it matters:** capability review can diverge by entry point, violating
  the graph as the source of truth and making security behavior harder to
  audit.
- **Improvement:** centralize candidate capability/trust diff calculation in
  `mesh-core-module`; shell and CLI render the same structured result.
- **Test:** parity fixtures for required/optional/elevated/high capability
  changes through both callers.
- **Confidence:** high for duplicated logic. **Status:** new; related to
  existing package/CLI transaction backlog.

### S02-DEAD-003 — Legacy fixture manifests remain in the repository

- **Source:** seven `config/modules/@mesh/*/package.json` files; active root
  uses `config/module.json` and the `../modules` tree.
- **Current behavior:** production loading rejects legacy `package.json`/
  `mesh.toml` inputs with migration diagnostics; these fixtures are not the
  active installed graph.
- **Why it matters:** stale data can be mistaken for a supported input or
  cause tooling/test drift.
- **Improvement:** retain only fixtures required to test rejection/migration,
  clearly mark their purpose, or remove after confirming no test/tool consumer.
- **Test:** canonical loader rejection and fixture inventory check.
- **Confidence:** confirmed stale, not a live runtime defect. **Status:**
  older audit/log; not a new backlog item.

## Logic and core mechanics

### S02-LOGIC-001 — Sparse root overlays can reactivate an inactive root

- **Source:** `package/profile.rs:51-62`, `package/composition.rs:328-344`.
- **Current behavior:** `ProfileRootInstance.active` defaults to `true`, and
  `merge_root` always assigns `base.active = overlay.active`. An overlay that
  omits `active` but changes `surface` therefore re-enables an inherited
  inactive root.
- **Why it matters:** a narrow profile edit can silently change the activation
  closure and show a surface the user disabled.
- **Improvement:** represent overlay presence separately from effective bool
  (`Option<bool>` or a dedicated sparse delta), then merge only present fields.
- **Test:** inactive composition root plus overlay containing only surface must
  remain inactive; explicit `active: true` must still enable it.
- **Confidence:** confirmed. **Status:** new.

### S02-LOGIC-002 — Explicit empty resource chains cannot clear inherited chains

- **Source:** `profile.rs:68-78`, `composition.rs:290-303`, and graph
  semantics documented in `installed_graph/graph.rs:385`.
- **Current behavior:** resource chains use default-empty `Vec`s and merge only
  when non-empty, so `{resources:{icons:[]}}` is indistinguishable from
  omission and inherits the base chain.
- **Why it matters:** composition/profile selection cannot express the
  specified “select no packs” decision; fallback order remains unexpectedly
  active.
- **Improvement:** preserve field presence with `Option<Vec<String>>` or an
  explicit delta type; distinguish absent, non-empty, and explicit empty.
- **Test:** base chain plus explicit empty icons/fonts/languages, plus omitted
  chain, and assert distinct resolved selections.
- **Confidence:** confirmed. **Status:** new.

### S02-LOGIC-003 — Install/uninstall graph validation can use the old store generation

- **Source:** `installed_graph/load.rs:46-60`, shell package paths at
  `shell/package.rs:77,126,148,316,329`, CLI paths at `main.rs:1054,1090`,
  store publication at `transaction.rs:447`.
- **Current behavior:** auto-discovered roots use the active immutable store
  when root inventory is empty. Install/uninstall mutates source and validates
  the graph before the new lock/store generation is published.
- **Why it matters:** a second install can be absent from discovery, while an
  uninstall can validate/commit a graph still containing the removed module.
- **Improvement:** construct candidate graphs against a staged candidate
  store/lock, or publish a candidate generation inside the transaction before
  validation with journal rollback.
- **Test:** with an empty root inventory and an existing active store, install a
  second module and uninstall a live module; assert candidate and committed
  graphs match the intended source set.
- **Confidence:** high. **Status:** new.

### S02-LOGIC-004 — Forced uninstall ignores profile node-slot references

- **Source:** `profile.rs:349-365,494-502`; uninstall callers in shell
  `package.rs:273` and CLI `main.rs:1521`.
- **Current behavior:** `references_module` checks roots, providers, services,
  and resource lists, but not `node_slots`, even though activation queues node
  slot contribution modules. Forced cleanup likewise does not remove these
  placements.
- **Why it matters:** a module used only by a slot can be removed, leaving a
  profile that later fails activation or retains a dangling placement.
- **Improvement:** parse contribution module IDs symmetrically in reference
  checks and `remove_module_references`.
- **Test:** module referenced only by an active and inactive node slot; verify
  refusal without force, complete cleanup with force, and successful reload.
- **Confidence:** confirmed. **Status:** new.

### S02-LOGIC-005 — Rollback can materialize the wrong Git source and profile

- **Source:** `transaction.rs:330-350`, rollback call sites in
  `crates/tools/cli/src/update.rs:927-1023`, lock composition metadata in
  `package/lock.rs:46`.
- **Current behavior:** `stage_locked_module` prefers an existing installed
  directory over the locked Git URL, so it may not contain the historical
  revision. Rollback restores modules/lock but does not restore the historical
  composition/profile pointer; it only removes references to absent modules.
- **Why it matters:** a successful-looking rollback can contain the wrong
  source revision and leave the active profile selecting newer composition
  decisions.
- **Improvement:** materialize the exact locked object or fetch its locked URL/
  revision; journal and restore composition/profile pointer as part of the
  lock generation.
- **Test:** two-revision remote fixture with a changed local checkout, and two
  composition generations; assert source digest, lock, profile, and active
  pointer all match the target generation.
- **Confidence:** high for source selection; medium-high for profile omission.
  **Status:** new.

### S02-LOGIC-006 — Package lock symlinks are followed before validation

- **Source:** `transaction.rs:182-195` and recovery `:264-278`.
- **Current behavior:** `.mesh-package.lock` is opened directly with
  `OpenOptions` before symlink metadata is rejected, unlike other contained
  paths.
- **Why it matters:** a symlink can redirect lock creation/advisory locking
  outside the configuration directory, weakening package-state containment.
- **Improvement:** inspect `symlink_metadata` and reject symlinks before open,
  or use a no-follow primitive with explicit regular-file validation.
- **Test:** Unix `begin` and `recover` fixtures with a symlinked lock path;
  verify no external target is created or locked.
- **Confidence:** confirmed. **Status:** new.

### S02-LOGIC-007 — Dangling active-profile pointers silently mean “none”

- **Source:** `package/profile.rs:748-758`.
- **Current behavior:** `active_profile_id` checks `path.exists()` before
  `validate_regular_file`; a dangling symlink therefore returns `Ok(None)`.
- **Why it matters:** a corrupt or redirected active-profile pointer can be
  treated as a clean no-profile state, changing startup behavior silently.
- **Improvement:** inspect link metadata first and reject symlinks/dangling
  pointers with a structured diagnostic.
- **Test:** dangling and external symlink fixtures must fail closed, while a
  missing regular pointer still returns `None`.
- **Confidence:** confirmed. **Status:** new.

### S02-LOGIC-008 — Same-version manifest/contribution edits can be invisible to graph diff

- **Source:** `installed_graph/graph.rs:1123-1144` and shell reconciliation
  at `crates/core/shell/src/shell/profile.rs:737`; separate authoring digest at
  `package/authoring.rs:66-74`.
- **Current behavior:** `InstalledModuleGraph::diff` marks updates only for
  kind, path, manifest version, or trust. Dependency, capability, entrypoint,
  interface, and contribution changes under the same ID/version can leave the
  diff empty, so reconciliation can skip activation.
- **Why it matters:** editable modules can change behavior without a graph
  generation transition or runtime refresh.
- **Improvement:** include a canonical manifest/contribution/tree fingerprint
  in graph identity/diff, with explicit source revisions for authoring edits.
- **Test:** same-version changes to capabilities, entrypoints, providers,
  resources, and contributions must produce a diff and activate the candidate.
- **Confidence:** high. **Status:** new.

### S02-LOGIC-009 — Installation spec contradicts the module-system activation rule

- **Source:** `docs/spec/02-installation.md:218-226` says a missing required
  interface provider leaves a frontend loaded but unavailable;
  `docs/spec/01-module-system.md:807-810` says missing required contracts
  reject the candidate; implementation blocks frontends in
  `installed_graph/graph.rs:169-201`.
- **Current behavior:** the two authoritative specification parts describe
  incompatible shipped effects, and the graph follows the rejection/blocking
  path.
- **Why it matters:** installers, graph diagnostics, health/UI behavior, and
  tests cannot share one contract for required services.
- **Improvement:** reconcile the specs explicitly, then align graph health and
  frontend activation to the chosen policy. This is a contract correction, not
  a reason to report the Target automation/health designs as bugs.
- **Test:** one module-owned fixture should assert the selected behavior for a
  missing required provider and preserve unrelated modules.
- **Confidence:** confirmed contract contradiction. **Status:** new.

### S02-LOGIC-010 — One invalid module can abort the whole discovery result

- **Source:** `crates/core/shell/src/shell/discovery.rs:2515-2532`, recovery
  handling in `discovery.rs:2124` and `crates/core/shell/src/shell/runtime/mod.rs:349`.
- **Current behavior:** the module-resolution loop propagates an individual
  `resolve_modules` error with `?`; the surrounding recovery path treats that
  failure as a discovery failure instead of preserving the valid modules.
- **Why it matters:** one malformed or incompatible module can remove an
  otherwise usable shell activation set, contrary to MESH's failure-isolation
  requirement.
- **Improvement:** return per-module diagnostics and a bounded partial graph;
  make only dependency-closure members unavailable, while keeping unrelated
  modules active and exposing the degraded state.
- **Test:** mix one invalid module with two independent valid modules and
  assert the valid modules resolve, activate, and report the isolated failure.
- **Confidence:** high. **Status:** new.

### S02-LOGIC-011 — Activation snapshots do not bind lock identity to content

- **Source:** `package/content_store.rs:122-155,702-727`.
- **Current behavior:** activation can select a content-store object using the
  lock key/path, but publication does not revalidate that the stored manifest
  identity and version match the lock record selected for activation.
- **Why it matters:** stale or mismatched store metadata can be activated as a
  module that appears to satisfy the lock, undermining reproducibility and
  making rollback provenance ambiguous.
- **Improvement:** validate module ID, version, source revision, and tree
  digest against the lock record before publication; record the verified
  identity in the activation generation.
- **Test:** inject an object whose manifest differs from its lock key and
  assert publication rejects it without changing the active generation.
- **Confidence:** medium-high; exact threat depends on all object-ingestion
  callers. **Status:** new.

### S02-LOGIC-012 — Discovery silently drops unreadable module entries

- **Source:** `package/installed_graph/load.rs:280-315`.
- **Current behavior:** metadata, canonicalization, or directory-read failures
  are converted to a skipped entry while discovery continues, without a
  durable module diagnostic tied to the omitted path.
- **Why it matters:** a permission or filesystem failure can look like a
  deliberate uninstall; the user and recovery logic cannot distinguish an
  incomplete graph from an empty one.
- **Improvement:** preserve a structured discovery diagnostic and source path
  in the candidate graph, while continuing with unrelated entries where safe.
- **Test:** unreadable and disappearing module directories must produce a
  visible diagnostic and never silently remove the prior active module.
- **Confidence:** high. **Status:** new.

### S02-LOGIC-013 — Immutable package generations have no bounded reclamation path

- **Source:** `package/content_store.rs:122-155,702-727` and transaction
  publication/recovery paths in `package/transaction.rs`.
- **Current behavior:** immutable objects and activation generations are
  retained across publication and rollback; lock history has retention logic,
  but the corresponding content and activation cleanup policy is not present
  in the reviewed package path.
- **Why it matters:** repeated editable installs or updates can grow disk use
  without a bounded policy, eventually making installation or recovery fail.
- **Improvement:** add a generation-aware garbage collector that retains the
  active generation, rollback window, and in-flight journal references, then
  deletes only unreferenced objects transactionally.
- **Measurement/test:** synthetic update history with known tree sizes; track
  disk growth, recovery correctness, and concurrent publication/GC races.
- **Confidence:** medium; verify other maintenance owners before removal.
  **Status:** new.

## Existing backlog or audit overlap

The older Section 02 findings on path escape, fail-open graphs, split package
transactions, compatibility enforcement, composition resolution, canonical
manifest loading, lifecycle health, and lock metadata were checked against the
current source and August log. The principal fixes are present and are not
repeated as current defects. Legacy `config/modules` content is stale fixture
material, not an accepted compatibility input. Section 15/16 backlog items
still cover unified package transactions and live activation acknowledgements;
the new findings above expose additional graph/profile/rollback seams that
should be linked during final reconciliation.

## Refuted suspicions

- Current path containment, full-tree validation, canonical manifest rejection,
  lock schema v3, transaction journaling, and active-store publication refute
  the older claims that every path/graph/lock operation is broadly unguarded.
- Parallel manifest loading was measured historically at 2.60–2.86× for its
  own stage but only 1.01–1.02× for complete graph startup; no new speedup is
  claimed here.
- Rejected `Arc<str>` and private Lua-table cache experiments were not retried.

## Tests and benchmarks needed

- Sparse overlay matrix for `active`, resource chains, provider aliases, and
  profile node slots; graph diff tests for same-version manifest changes.
- Candidate-store install/uninstall, exact Git rollback, composition/profile
  rollback, lock symlink, and dangling active-pointer tests.
- Resolve and document the required-provider policy conflict, then run the
  focused module suite under the chosen contract. The agent run reported
  `274 passed, 9 failed, 3 ignored`; those failures matched the existing
  August baseline and were not changed by this audit.
- Release benchmarks for shell-loop blocking, whole-state snapshot I/O,
  authoring refresh hashing, manifest/tree passes, and oversized source limits,
  each with workload sizes, repeated runs, and recovery/correctness gates.

## File coverage

**Assigned:** 72/72 files: all files under
`crates/core/extension/module/`, seven `config/modules/**/package.json`
fixtures, shipped non-interface manifests, and the three module/installation
spec documents. **Inspected:** 72/72. **Excluded from this section:** callers
outside the assigned roots were searched but belong to their owning sections;
global build/Git/archive/planning-history/binary exclusions are listed in
`../00-coverage.md`. **Files still needing review:** none.
