# Section 01 — Core foundation contracts

## Scope and coverage

Reviewed the assigned foundation contracts, settings document, and their
repository-wide consumers. The section includes the capability, config,
debug, and diagnostics crates, `config/settings.json`, and
`docs/spec/08-settings.md`. All **14/14 assigned files** were inspected;
neighboring shell, module, runtime, surface-config, CLI, and LSP callers were
also searched. No section file remains to review.

## Process tree

```text
module/profile/settings input
  -> parse and normalize JSON
  -> owner/schema validation and sparse projection
  -> effective settings + capability policy
  -> shell/runtime/module consumers
  -> diagnostics and debug snapshot
  -> serialized debug/service payloads
  -> durable CAS commit and revision publication
```

The in-memory schema replacement and profile preparation paths have candidate
boundaries. Durable settings/profile revision check plus write is still a
check-then-write sequence. Diagnostics and debug snapshots are assembled from
multiple independently locked/copying views. Capability resolution exists in
normal discovery, but a few legacy/test-facing fallbacks and unconsumed proof
types remain.

## Performance findings

### S01-PERF-001 — Debug telemetry is rebuilt and serialized on every enabled frame

- **Source:** `crates/core/shell/src/shell/runtime/render/mod.rs:82-85`,
  `crates/core/shell/src/shell/runtime/debug.rs:19-78,218-293`.
- **Current behavior:** every frame with debug enabled reconstructs graph,
  module, surface, diagnostics, profiling, and settings-derived data, then
  serializes it. The profiling stream and Chrome trace also derive from the
  same snapshot path.
- **Why it matters:** idle shell frames pay graph traversal, cloning, sorting,
  JSON allocation, and diagnostics work proportional to installed modules,
  surfaces, and issue history. The magnitude is not measured in an
  end-to-end workload.
- **Improvement:** cache immutable debug payloads by the relevant graph,
  settings, diagnostics, profiling, and surface generations, and publish on a
  bounded cadence or only when a consumer is present.
- **Measurement:** release runs under `nix develop`, 1,000 frames with
  16/128/512 modules, 4/32 surfaces, profiling on/off, and 0/100 diagnostics;
  measure p50/p95 frame time, allocations, payload bytes, and publication
  count before/after.
- **Confidence:** confirmed behavior, impact hypothesis. **Status:** already
  in backlog (`S01-PERF-002`); related to the older Section 01 audit.

### S01-PERF-002 — Diagnostics snapshot work is repeated and not shared

- **Source:** `crates/core/foundation/diagnostics/src/lib.rs:453-500,655-688`,
  `crates/core/shell/src/shell/runtime/debug.rs:60-77`.
- **Current behavior:** collector snapshots call `health`, `issues`, and
  `active_issues` separately; `active_issues` calls `issues` again. The shell
  requests a diagnostics snapshot twice while constructing one debug snapshot.
- **Why it matters:** multiple locks, clones, sorts, and health string
  construction scale with retained issue history and can produce a mixed-time
  view under concurrent updates.
- **Improvement:** expose one locked point-in-time snapshot containing active,
  historical, and aggregate views, then reuse it through debug publication.
- **Measurement:** 128 modules × 4 instances with 0/10/100 issues, recording
  lock acquisitions, allocations, elapsed time, and serialized payload parity.
- **Confidence:** confirmed. **Status:** already in backlog
  (`S01-PERF-003 / S01-DEAD-011`).

### S01-PERF-003 — Settings and profile durable writes remain synchronous and over-read

- **Source:** `crates/core/foundation/config/src/settings.rs:458-492`,
  shell theme/settings consumers, and the profile writer in
  `crates/core/extension/module/src/package/profile.rs:810`.
- **Current behavior:** `save_if_revision` rereads the backing document, then
  clones, serializes, fsyncs, renames, and fsyncs the parent. Preparation can
  load/validate settings before this path, and the shell-loop caller performs
  the durable operation synchronously.
- **Why it matters:** settings/theme/locale/profile changes can block frame
  processing and the check/read/write sequence does not provide an atomic
  writer decision.
- **Improvement:** use one serialized storage transaction/CAS boundary and
  move durable I/O off the shell loop while preserving commit acknowledgement.
- **Measurement:** 100/1,000 namespaces, 64 KiB/1 MiB files, 100/1,000
  mutations, and concurrent writers; measure parse count, fsync/syscall count,
  p50/p95 mutation latency, frame lateness, and conflict rate.
- **Confidence:** confirmed. **Status:** already in backlog
  (`S01-LOGIC-003 / S01-PERF-001`).

### S01-PERF-004 — Schema projection and capability grant copies need measurement before redesign

- **Source:** `config/src/settings.rs:291-331,513-530`,
  `config/src/validate.rs:342-380`, capability resolution in
  `crates/core/foundation/capability/src/lib.rs:250-285`, and runtime grant
  adapters in shell/backend construction.
- **Current behavior:** schema registration/rebuild copies the store and
  validates whole registered namespaces; validation clones accepted values and
  paths. Activation constructs owned sets, then frontend/backend creation
  converts them into additional sets/vectors.
- **Why it matters:** activation, profile switches, and restarts scale with
  all namespaces and granted capabilities, but there is no current benchmark
  establishing user-visible cost.
- **Improvement:** benchmark first; only then consider borrowed schema maps,
  incremental validation, or shared immutable effective grants.
- **Measurement:** schema: 1/32/256 namespaces × 8/32/128 fields over repeated
  reloads; grants: 500 modules × 10 grants × 4 roots and 50 restarts. Record
  allocations, retained bytes, and activation/restart latency.
- **Confidence:** confirmed allocation behavior, speculative severity.
  **Status:** already in backlog (`S01-PERF-005`, `S01-PERF-006`).

### S01-PERF-005 — Allocation-profiler coverage is too narrow

- **Source:** `crates/core/foundation/debug/src/allocation.rs:162-203,283-334`.
- **Current behavior:** the counting path adds atomic work to allocations, but
  the existing benchmark is a direct 64-byte allocation microbenchmark.
- **Why it matters:** frame-level overhead of the enabled allocator is unknown;
  a microbenchmark does not establish idle/render/reload impact.
- **Improvement:** add representative retained-render and shell workloads to
  the gate before optimizing the allocator path.
- **Measurement:** release idle, scroll, theme reload, and 1,026-node retained
  render with and without allocation profiling; compare p95 frame time,
  allocations, and CPU samples across repeated runs.
- **Confidence:** confirmed measurement gap. **Status:** already in backlog
  (`S01-PERF-004`).

## Dead code and redundancy

### S01-DEAD-001 — Diagnostics dependencies are unused

- **Source:** `crates/core/foundation/diagnostics/Cargo.toml:8-12` and
  `diagnostics/src/lib.rs`.
- **Current behavior:** direct `tracing`, `tracing-subscriber`, and `thiserror`
  dependencies are declared, while the crate source uses `serde` for its
  current implementation.
- **Why it matters:** stale dependencies obscure ownership and increase
  dependency/build surface.
- **Improvement:** remove after public-crate/API review; verify the full
  workspace and dependency tree.
- **Test:** `cargo check --workspace`, focused diagnostics tests, and
  `cargo tree -p mesh-core-diagnostics`.
- **Confidence:** confirmed repository-dead. **Status:** already in backlog
  (`S01-DEAD-007`, older Section 01 audit).

### S01-DEAD-002 — Compatibility and parallel APIs remain unconsumed

- **Source:** `diagnostics/src/lib.rs:90-98,145-154,427-450`;
  `debug/src/lib.rs:88-94,665-719,742-749`;
  `config/src/lib.rs:573-588,483-529`;
  `config/src/settings.rs:291-338`; capability helpers at
  `capability/src/lib.rs:21-53,207-213,246-248,340-376`.
- **Current behavior:** `ModuleMetrics`, `LifecycleErrorRecord` projection,
  legacy `DebugTab` state, `ConfigError::Validation`, incremental schema
  registration, legacy privilege classifiers, capability proof handle, and
  several capability introspection accessors have no repository production
  consumers. Complete schema replacement and current `DebugInspectorView` are
  the active paths.
- **Why it matters:** parallel public models can drift and make removal or
  security review ambiguous.
- **Improvement:** retain only deliberately supported external adapters;
  otherwise privatize/remove each after downstream API review.
- **Test:** workspace compile, graph schema replacement/rollback tests,
  debug active-view serialization, and activation capability propagation.
- **Confidence:** confirmed repository-unconsumed; external compatibility risk
  remains. **Status:** already in backlog (`S01-DEAD-002/003/005/006/008/009/012/013`).

### S01-DEAD-003 — Frontend graph schema and surface-policy schema duplicate ownership

- **Source:** shell graph schema registration in
  `crates/core/shell/src/shell/discovery.rs:1525-1585,1628-1685` and
  `crates/core/surface-config/src/lib.rs:690-760`.
- **Current behavior:** both paths declare overlapping `surface`, `props`,
  `icons`, and `i18n` field knowledge. Graph registration assembles owner
  schemas while surface-config performs additional semantic validation.
- **Why it matters:** enum fields and role-specific policy can diverge between
  graph validation and later runtime policy resolution, creating inconsistent
  diagnostics and duplicated maintenance.
- **Improvement:** keep graph registration as assembly/ownership, but derive
  frontend field definitions and canonical enum validators from one contract
  owner.
- **Test:** schema-key/enum parity plus invalid anchor, layer, keyboard-mode,
  and role-field fixtures through both graph and surface-config paths.
- **Confidence:** high for duplication, medium for current user impact.
  **Status:** new; related to Section 13’s surface-policy findings.

## Logic and core mechanics

### S01-LOGIC-001 — Per-instance settings can bypass the validated owner projection

- **Source:** `crates/core/foundation/config/src/settings.rs:267-279,502-530`.
- **Current behavior:** rebuild validates `@module#instance` using the base
  schema, but `namespace()` resolves the base through validated storage and
  the instance through `resolved_stored(name)`. Because the instance schema is
  not registered under its literal name, the instance can fall back to raw
  JSON. The existing test at `settings.rs:943-965` checks raw preservation but
  not invalid runtime filtering.
- **Why it matters:** malformed per-instance values can reach runtime despite
  the sparse settings contract requiring invalid values to be omitted while
  retaining raw data for repair.
- **Improvement:** resolve instance layers from `validated_root` using the base
  owner schema; expose raw values only to repair/doctor paths.
- **Test:** invalid typed value under `@scope/name#instance` is absent from
  `namespace()` but remains in `to_value()` and has a diagnostic.
- **Confidence:** confirmed. **Status:** already in backlog (`S01-LOGIC-002`).

### S01-LOGIC-002 — Durable revision checks are TOCTOU rather than serialized CAS

- **Source:** `settings.rs:458-492`, profile save path at
  `crates/core/extension/module/src/package/profile.rs:810`, and CLI profile
  mutations at `crates/tools/cli/src/main.rs:894,907,1906`.
- **Current behavior:** each writer checks a revision and later atomically
  replaces the file. Two writers can both observe revision N and both commit
  N+1; `saturating_add` also allows revision exhaustion to reuse MAX.
- **Why it matters:** atomic replacement prevents torn JSON but not lost
  updates or an authoritative generation decision.
- **Improvement:** serialize the read/check/write/rename boundary, use checked
  increment semantics, and route every mutation through it.
- **Test:** barrier-synchronized same-revision writers yield one success and
  one conflict; verify committed revision increments once. Include profile and
  CLI mutations.
- **Confidence:** confirmed. **Status:** already in backlog
  (`S01-LOGIC-003 / S01-PERF-001`).

### S01-LOGIC-003 — Shipped debug inspector requests an unknown capability

- **Source:** capability catalog `crates/core/foundation/capability/src/lib.rs:104-146`;
  `modules/frontend/debug-inspector/module.json:12-17` declares
  `service.debug.control`; root approval repeats it in `config/module.json:33`.
  Shell debug operation policy names the same capability in
  `crates/core/shell/src/shell/service.rs:49`.
- **Current behavior:** the closed catalog includes `service.debug.read` but
  not `service.debug.control`; discovery therefore rejects the shipped module
  as `UnknownCapability` before activation.
- **Why it matters:** a shipped debug surface cannot activate through the
  normal graph path, and the intended read/control privilege split is
  contradictory.
- **Improvement:** either add the control capability with an explicit policy
  and align operations/approvals, or remove the declaration and make debug
  controls intentionally read-authorized. Do not silently accept unknown ids.
- **Test:** enable the shipped debug module in a graph fixture and assert the
  exact resolved capability set and successful activation.
- **Confidence:** confirmed. **Status:** new.

### S01-LOGIC-004 — Invalid array members are compacted instead of rejecting the field

- **Source:** `crates/core/foundation/config/src/validate.rs:342-380` and
  pack-chain schemas registered from shell discovery.
- **Current behavior:** generic array validation drops invalid members and
  returns the remaining values. A pack list such as `["pack-a", 7,
  "pack-b"]` becomes `["pack-a", "pack-b"]`.
- **Why it matters:** an explicitly ordered wholesale replacement silently
  changes meaning, contrary to the settings rule that an invalid override
  falls back without destroying the valid lower layer.
- **Improvement:** reject the entire array unless its schema explicitly opts
  into filtering semantics; preserve raw data for diagnostics/repair.
- **Test:** mixed-type icon/font/locale pack chains must produce a diagnostic
  and omit the override without compacting it.
- **Confidence:** confirmed. **Status:** new; related to the settings contract
  and resource sections.

### S01-LOGIC-005 — Tooltip runtime reads durable settings instead of effective settings

- **Source:** `crates/core/shell/src/shell/component/shell_component/internals.rs:48-60,132-135`;
  effective store ownership in `shell/component.rs:720-728` and replacement in
  `shell_component/mod.rs:1416-1425`.
- **Current behavior:** hover/tick/paint paths call `load_shell_settings()`
  instead of using the already-held effective `SettingsStore`. Profile-scoped
  overrides can be ignored, stale file values can win after a load failure,
  and file I/O is reachable from interaction/render work.
- **Why it matters:** settings precedence and profile switching can disagree
  with tooltip behavior; the path also adds avoidable blocking I/O.
- **Improvement:** read the effective store and update tooltip policy only when
  the store revision changes.
- **Test:** switch profile-scoped tooltip settings without rewriting the shared
  file; verify immediate behavior and no per-frame settings load.
- **Confidence:** confirmed. **Status:** new; overlaps Section 15 settings
  propagation and the Section 01 durable-I/O backlog.

### S01-LOGIC-006 — Lifecycle diagnostic helper chooses an arbitrary instance

- **Source:** `crates/core/foundation/diagnostics/src/lib.rs:539-653`.
- **Current behavior:** when a lifecycle error lacks an explicit instance,
  resolution searches the collector and can attach it to the first matching
  `(module_id, instance_id)` in map order. With multiple instances that is not
  necessarily the failing provider.
- **Why it matters:** health/debug attribution and resolution can affect the
  wrong instance, violating observable lifecycle identity.
- **Improvement:** require module/instance/generation identity; allow the
  legacy helper only for exactly one matching instance, otherwise reject or
  emit an ambiguity issue.
- **Test:** two instances, record/resolve one lifecycle stage, assert the other
  instance is unchanged.
- **Confidence:** confirmed. **Status:** already in backlog (`S01-LOGIC-007`).

### S01-LOGIC-007 — Diagnostics history and snapshots have no bounded contract

- **Source:** `diagnostics/src/lib.rs:156-160,200-239,453-500,655-688` and
  debug export at `shell/runtime/debug.rs:60-77`.
- **Current behavior:** arbitrary issue codes/messages remain in a map after
  resolution; health joins all active messages and debug includes history and
  active issue data.
- **Why it matters:** a faulty/hostile extension can grow memory, strings, and
  debug/IPC payloads without an uptime or per-instance bound.
- **Improvement:** bound issue count and code/message bytes, bound health and
  debug payloads, and emit one deterministic overflow diagnostic.
- **Test:** inject 100,000 unique issues and oversized messages while taking
  snapshots; verify memory and payload size plateau and overflow is observable.
- **Confidence:** confirmed. **Status:** already in backlog (`S01-LOGIC-006`).

### S01-LOGIC-008 — Diagnostics snapshots can mix generations

- **Source:** `diagnostics/src/lib.rs:655-688`, shell debug collection at
  `runtime/debug.rs:60-77`.
- **Current behavior:** health, historical issues, and active issues use
  separate locks/copies; issue resolution can occur between those reads.
- **Why it matters:** one payload can claim a health state that does not match
  its active issue list, while also repeating work.
- **Improvement:** generate one locked per-instance snapshot and derive all
  aggregate/debug fields from that immutable view.
- **Test:** concurrent record/resolve with snapshot consistency assertions;
  benchmark 100–1,000 issues for lock/allocation cost.
- **Confidence:** confirmed. **Status:** already in backlog
  (`S01-PERF-003 / S01-DEAD-011`).

### S01-LOGIC-009 — Debug module and surface ordering is nondeterministic

- **Source:** `shell/runtime/debug.rs:24-38,137-143`.
- **Current behavior:** module and visible-surface vectors are collected from
  hash-map iterators without sorting, unlike other debug collections.
- **Why it matters:** identical state can serialize differently, breaking
  stable snapshot tests and downstream cache/diff consumers.
- **Improvement:** sort by stable module/instance and surface identity, with
  generation as a tie-breaker where needed.
- **Test:** construct equivalent states in different insertion orders and
  compare serialized payloads.
- **Confidence:** confirmed. **Status:** already in backlog (`S01-LOGIC-005`).

## Refuted suspicions

### Refuted or deliberately not promoted

- The old raw shell-event capability bypass is not a current finding: normal
  discovery now has catalog-backed policy and the typed operation path; the
  remaining `None` capability fallbacks are test-facing callers in current
  source. Keep the fail-closed cleanup in the existing backlog, but do not
  describe it as an active production bypass without new evidence.
- Profiling sample retention is bounded, and `profiling_available()` checks
  actual allocator installation. The older concerns about an unbounded sample
  ring and feature-only availability are refuted.
- The rejected `SmallVec`, scratch-map, `Vec::drain`, `Arc<str>`, and detached
  Lua-table cache experiments were not repeated. No speedup is claimed from
  those approaches.

## Existing backlog or audit overlap

The carried Section 01 findings are linked in the status/backlog rather than
duplicated as new work: `S01-LOGIC-002/003/006/007`,
`S01-PERF-001/003/004/005/006`, and `S01-DEAD-002/003/004/005/006/007/008/009/010/011/012/013/014/015`.
The older Section 01 report and August performance log are historical evidence;
the report above only promotes current evidence or labels an item as overlap.
New candidates from this pass are the shipped debug capability mismatch,
frontend/surface schema duplication, invalid-array compaction, and tooltip
durable-settings read. Section 13 and Section 15 should check those seams
before backlog reconciliation.

## Tests and benchmarks needed

- Per-instance schema filtering, array rejection, tooltip effective-store
  propagation, two-writer CAS, revision overflow, and multi-instance
  lifecycle identity.
- Debug snapshot deterministic ordering and diagnostics health/history/active
  parity under concurrent updates.
- Debug publication benchmark with graph/surface/diagnostic cardinalities;
  diagnostics snapshot lock/allocation benchmark; schema validation scaling;
  capability grant-copy scaling; and frame-level allocation-profiler workload.
- Shipped debug-inspector graph activation fixture with exact capability policy.

## File coverage

**Assigned:** all 14 files: the four foundation package manifests and source
files/tests under `crates/core/foundation/{capability,config,debug,diagnostics}/`,
`config/settings.json`, and `docs/spec/08-settings.md`. **Inspected:** 14/14.
**Excluded from this section:** other package files were not assigned here but
were searched as callers/seams; build output, Git data, frozen planning archive,
planning history, the audit output, and binary assets are globally excluded as
documented in `00-coverage.md`. **Files still needing review:** none.
