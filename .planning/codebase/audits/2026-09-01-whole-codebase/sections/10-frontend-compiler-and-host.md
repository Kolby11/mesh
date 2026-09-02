# Section 10 — Frontend compiler and host

## Scope and coverage

Reviewed all 68 assigned files in the frontend ABI/compiler/host and expression
packages, assigned `.mesh` sources, frontend fixtures, and frontend contract
documents. Component, element, runtime, service, shell, render, and LSP
callers were searched. **68/68 assigned files inspected; no follow-up remains.**

## Process tree

```text
module graph/manifest/profile/source
  -> safe entrypoint/import resolution and parse
  -> import/slot/props/service/capability validation
  -> compiled roots, reverse dependencies, watches, revision
  -> host/runtime context and snapshots
  -> Luau/expression evaluation -> WidgetNode tree/effects
  -> style/layout/render and shell-facing requests
  -> hot reload, catalog replacement, failure recovery, teardown
```

## Performance findings

### S10-PERF-001 — Tree construction and style preparation repeat work per rebuild

- **Source:** `crates/core/frontend/compiler/src/render/mod.rs:70-190,457-738`
  and prepared style context callers.
- **Current behavior:** a runtime rebuild traverses the component tree, lowers
  attributes/expressions, and resolves style/props for all nodes.
- **Why it matters:** a service field or local state change can make a large
  frontend pay full tree construction rather than a bounded subtree update.
- **Recommended improvement:** retain compiled/static nodes and dependency-index
  dynamic expressions, rebuilding only affected subtrees; preserve explicit
  revision boundaries.
- **Measurement:** 100/1,000/10,000 nodes with leaf, repeated service, prop,
  locale, and theme changes; measure nodes rebuilt, allocations, CPU and p95
  frame gap.
- **Confidence:** medium hypothesis. **Status:** new; no rejected traversal
  experiment repeated.

### S10-PERF-002 — Recursive import compilation rereads unchanged components

- **Source:** `compiler/src/compile.rs:176-288,427-629`.
- **Current behavior:** entrypoint compilation recursively parses local/imported
  components and constructs dependency/watch collections.
- **Why it matters:** hot reload and LSP edits can reread the complete reachable
  graph for a leaf change.
- **Recommended improvement:** cache parsed component artifacts by content
  digest and invalidate reverse dependents only.
- **Measurement:** 10/100/1,000 component graphs, leaf versus root edits; record
  files read, parses, allocations, p50/p95 compile time, and invalidation set.
- **Confidence:** high repeated traversal; impact unmeasured. **Status:** new.

### S10-PERF-003 — Effect/state observation summaries can copy broad values

- **Source:** `frontend/src` ABI effect batches and host/runtime observation
  publication.
- **Current behavior:** effects and service observations cross the host boundary
  as owned values/batches, with per-revision summaries for reconciliation.
- **Why it matters:** frequent service/input updates can clone payloads even
  when no component consumes the changed field.
- **Recommended improvement:** use immutable payload handles and dependency-keyed
  change summaries; retain ownership isolation and bounded queues.
- **Measurement:** 100 components, 1/10/100 service fields, 60/240 Hz updates;
  measure clone bytes, allocations, queue depth, consumer rebuilds, and latency.
- **Confidence:** medium hypothesis. **Status:** new.

## Dead code and redundancy

### S10-DEAD-001 — Frontend ABI effect types and shell request mapping are parallel schemas

- **Source:** `frontend/abi/src/lib.rs:149-326` and shell effect/request
  adapters.
- **Current behavior:** typed frontend effects are converted into shell-specific
  requests and policy decisions in another layer.
- **Why it matters:** new effect variants can be accepted by ABI but silently
  ignored or differently authorized by shell adapters.
- **Recommended improvement:** keep ABI renderer/host-neutral and derive a
  single exhaustive adapter mapping with compile-time tests; remove wrappers
  only after call-site audit.
- **Test:** exhaustive effect matrix, unknown/rejected effect behavior, and
  capability parity through runtime and shell.
- **Confidence:** high duplication; **Status:** older audit/backlog overlap.

### S10-DEAD-002 — Compiler, component, and LSP repeat import/symbol analysis

- **Source:** compiler `compile.rs` import validation, component
  `parser/script.rs`, and LSP component analyzers.
- **Current behavior:** each consumer derives aliases, member accesses, source
  spans, and dependencies from related source/AST data.
- **Why it matters:** diagnostics and runtime dependency invalidation can
  disagree, and maintenance fixes must be applied in several owners.
- **Recommended improvement:** make the component AST/semantic index canonical;
  use explicit tooling recovery adapters rather than re-parsing production
  syntax.
- **Confidence:** confirmed redundancy; **Status:** older audit/Section 07.

### S10-DEAD-003 — Expression compile/evaluation and preview semantics overlap

- **Source:** `crates/core/ui/expression/src/lib.rs:19-170` and compiler/host
  expression consumers.
- **Current behavior:** preview and live paths use related but separate value
  conversion/evaluation boundaries.
- **Why it matters:** an expression can preview successfully but fail or coerce
  differently in the live Luau/runtime path.
- **Recommended improvement:** compile one bounded IR and expose explicit live
  versus preview capabilities through one evaluator.
- **Test:** literals, Unicode, tables, service/props access, errors, and type
  coercion parity.
- **Confidence:** medium-high. **Status:** older audit.

## Logic and core mechanics

### S10-LOGIC-001 — All import and entrypoint reads need one safe module-root resolver

- **Source:** `compiler/src/compile.rs:209-288,597-646` and component script
  import parsing.
- **Current behavior:** multiple paths resolve entrypoints/local imports/module
  components and watchers; they must agree on canonical containment, symlinks,
  regular files, and source-root ownership.
- **Why it matters:** a path that compiles in one path but is watched/diagnosed
  in another can escape module isolation or reload the wrong source.
- **Recommended improvement:** return a typed validated source handle from one
  resolver and reuse it for compilation, dependency graphs, watches, LSP, and
  diagnostics.
- **Test:** absolute/parent/symlink/path-swap, nested imports, missing files,
  and watched-path parity.
- **Confidence:** medium-high seam; current code has dedicated resolvers.
  **Status:** older audit/backlog overlap.

### S10-LOGIC-002 — Primary and contribution roots must share validation and lifecycle

- **Source:** `compile_frontend_module_revision` and contribution-root compile
  paths in `compiler/src/compile.rs:176-302`, catalog publication.
- **Current behavior:** primary roots and extension-point/contribution roots are
  compiled through related but distinct entrypoint and validation flows.
- **Why it matters:** a contribution can bypass interface, capability, slot,
  props, or source dependency checks applied to the primary root.
- **Recommended improvement:** represent every root as a typed frontend target
  with the same compile/validate/revision/lifecycle pipeline.
- **Test:** malformed contribution, missing interface/capability, reload, and
  profile removal alongside a primary root.
- **Confidence:** medium-high. **Status:** older audit/backlog overlap.

### S10-LOGIC-003 — Catalog revision, watches, and live instances must commit together

- **Source:** `frontend/src/lib.rs:144-257`, host catalog publication and shell
  reload/profile consumers.
- **Current behavior:** compiled revisions and runtime instances have distinct
  ownership; a source/catalog change can update one before reverse dependencies,
  watch set, and retained instances are reconciled.
- **Why it matters:** a stale component can survive a successful catalog update,
  or a failed compile can leave a watch/catalog mismatch.
- **Recommended improvement:** prepare an immutable catalog candidate containing
  roots, dependencies, watches, diagnostics, and generation; atomically swap it
  and reconcile instances by generation.
- **Test:** leaf/root edits, removed imports, contribution changes, failed
  compile, concurrent profile switch, and last-known-good recovery.
- **Confidence:** high architecture seam. **Status:** older audit/backlog overlap.

### S10-LOGIC-004 — Runtime props/publication failures need transactional behavior

- **Source:** compiler render prop lowering, host runtime publication, and
  component state consumers.
- **Current behavior:** Rust-side props, CSS `prop()`, Luau `props`, settings,
  and instance overrides cross separate update paths.
- **Why it matters:** one failed/coerced publication can leave Rust, Lua, style,
  and settings views disagreeing.
- **Recommended improvement:** validate a complete typed prop snapshot, then
  commit it once with a generation; retain prior values on failure and expose a
  diagnostic.
- **Test:** invalid higher-precedence value, structured values, defaults,
  localized metadata, script writes, and concurrent reload.
- **Confidence:** high. **Status:** older audit/backlog overlap.

### S10-LOGIC-005 — Frontend host effects need a narrow, capability-checked boundary

- **Source:** `frontend/abi/src/lib.rs:168-326` and host/shell effect adapters.
- **Current behavior:** host effects include service, surface, debug, and core
  requests; downstream shell adapters also own policy and capability decisions.
- **Why it matters:** a frontend can gain policy through an effect variant or
  observe authorization behavior that differs between direct and routed paths.
- **Recommended improvement:** keep effects declarative and typed, centralize
  capability authorization in the core, and make adapters exhaustive and
  generation-aware.
- **Test:** every effect kind with missing/revoked capability, stale generation,
  surface removal, and rejected side effect.
- **Confidence:** medium-high. **Status:** older audit/backlog overlap.

## Existing backlog or audit overlap

The prior frontend audit covers path safety, service isolation, hot reload,
contribution validation, expression parity, host policy, dependency invalidation,
alias/symbol analysis, prop publication, diagnostics, generation publication,
child surfaces, and lifecycle hooks. Current code includes typed effects,
revisioned catalogs, parser-owned spans, and shared CSS lowering; fixed claims
are not repeated. New candidates are shared invalidation indexes and workload
measurements.

## Refuted suspicions

- The current expression compiler explicitly rejects non-ASCII only where its
  contract requires it (`expression/src/lib.rs:108+`); a generic Unicode panic
  claim is not promoted without a current reproducer.
- No rejected cache/traversal optimization is repeated without a new benchmark.

## Tests and benchmarks needed

- Safe source handles, root parity, alias/cycle, expression, props, effects,
  capabilities, catalog generations, hot reload, and rollback tests.
- Compiler/rebuild benchmarks with graph/node/file/edit sizes, allocations,
  files parsed, nodes rebuilt, effect bytes, queue bounds, and p95 latency.

## File coverage

**Assigned:** 68/68 frontend ABI/compiler/host/expression files, assigned
frontend `.mesh` sources/fixtures, and frontend contract documents. **Inspected:**
68/68. Component/elements/runtime/Wayland callers were searched but belong to
Sections 07, 08, 11, and 14. **Files still needing review:** none.
