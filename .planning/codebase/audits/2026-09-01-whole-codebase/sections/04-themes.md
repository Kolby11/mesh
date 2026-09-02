# Section 04 — Themes

## Scope and coverage

Reviewed all 18 assigned files: the theme foundation crate, seven shipped theme
module files, and `docs/spec/04-styling.md` (the repository's authoritative
styling contract; there is no `docs/spec/04-themes.md`). Theme callers in shell,
component styling, animation, settings, service state, and graph contributions
were searched. **18/18 assigned files inspected; no follow-up remains.**

## Process tree

```text
theme module.json / CSS / profile and settings selection
  -> graph contribution and path validation
  -> parse and validate CSS, tokens, defaults, modes, keyframes
  -> compose effective immutable theme revision
  -> style/token/default lookup by component and element
  -> component/layout/render invalidation and theme service publication
  -> reload, diagnostics, last-known-good recovery, shutdown
```

The implementation still contains a direct filesystem/theme-engine path beside
the installed graph contribution model. That seam is more important than any
individual token lookup optimization because it affects provenance, security,
mode selection, and recovery.

## Performance findings

### S04-PERF-001 — Theme resolution can repeat full composition and cache invalidation

- **Source:** `crates/core/foundation/theme/src/lib.rs:391-430, 488-562` and
  shell reload/invalidation callers.
- **Current behavior:** active-theme changes and reloads rebuild theme state and
  invalidate broad component/render consumers, even when a change affects only
  one token or source file.
- **Why it matters:** large retained trees pay repeated token lookup, style
  resolution, layout, and repaint work during edits or mode changes.
- **Recommended improvement:** publish a content-addressed immutable theme
  snapshot, compute changed token/default/keyframe sets, and invalidate only
  dependent style/layout/render nodes.
- **Test/benchmark:** 100/1,000/10,000-node trees with one leaf token, one
  inherited token, and a full theme replacement; measure parse time, nodes
  resolved, damaged pixels, frame gap, and allocations over repeated reloads.
- **Confidence:** high behavior, impact unmeasured. **Status:** related to
  existing theme/render backlog; no rejected experiment repeated.

### S04-PERF-002 — Theme CSS parsing and validation duplicate source work

- **Source:** `theme/src/lib.rs:631-708, 800+` and `theme/src/css.rs`.
- **Current behavior:** comment/brace scanning, declaration parsing, selector
  storage, and later resolver interpretation are separate passes over text.
- **Why it matters:** editor reloads and startup reread large CSS files and
  retain intermediate strings/maps.
- **Recommended improvement:** lower once to a bounded AST with source spans,
  then share it for validation, token graph compilation, diagnostics, and
  resolution.
- **Measurement:** 7 shipped themes plus synthetic 1/10/100 KiB themes; count
  bytes read, allocations, parse passes, p50/p95 reload time, and diagnostics.
- **Confidence:** confirmed multiple passes; speedup is a hypothesis. **Status:**
  new measurement item.

### S04-PERF-003 — Token lookup and inherited custom-property state need workload data

- **Source:** `theme/src/lib.rs` token APIs and UI style resolution in
  `crates/core/ui/elements/src/style/resolve/value.rs`.
- **Current behavior:** token and custom-property lookups occur during style
  resolution, with per-node state and recursive references potentially revisited
  across a subtree.
- **Why it matters:** deeply nested component trees can amplify repeated string
  lookup and dependency traversal.
- **Recommended improvement:** compile a typed token dependency graph once per
  theme revision and pass inherited values through the cascade; cache only after
  measuring memory and invalidation behavior.
- **Measurement:** depth 10/50/200 trees, 10/100/1,000 tokens, alias chains of
  length 1/5/20; measure lookup count, CPU, allocations, and cache hit rate.
- **Confidence:** medium hypothesis. **Status:** new; not a rejected cache
  experiment without evidence.

## Dead code and redundancy

### S04-DEAD-001 — Direct theme metadata/parser path duplicates graph contributions

- **Source:** `theme/src/lib.rs:488-562` and canonical graph contribution indexes
  in `crates/core/extension/module/src/package/installed_graph/`.
- **Current behavior:** the shell/theme engine reads theme directories and a
  private theme metadata shape while the module graph already owns canonical
  module identity, ownership, paths, enablement, and provenance.
- **Why it matters:** two loaders can disagree on available themes and make
  private parser code appear supported after the canonical format changed.
- **Recommended improvement:** make a graph-derived theme descriptor the single
  runtime owner; keep the direct parser only as a rejection/migration fixture,
  then remove it after callers and fixtures are migrated.
- **Test:** repository-wide call graph, canonical manifest fixtures, duplicate
  IDs, disabled/removed modules, and path containment tests.
- **Confidence:** confirmed duplication, not dead until migration consumers are
  audited. **Status:** older audit/backlog overlap.

### S04-DEAD-002 — Theme engine stores overlapping active/catalog representations

- **Source:** `theme/src/lib.rs:198-220, 391-430`.
- **Current behavior:** the engine retains a mutable `Vec<Theme>` catalog plus a
  separate active selection, while graph/profile snapshots represent the same
  selection and source provenance.
- **Why it matters:** duplicate authority permits stale cached themes, duplicate
  IDs, and order-dependent selection.
- **Recommended improvement:** use one immutable revisioned catalog/snapshot and
  expose read-only resolved descriptors; retain compatibility adapters only at
  API boundaries.
- **Test:** duplicate IDs, same-ID edits, profile switch, reload failure, and
  catalog replacement tests; remove only after repository-wide consumer review.
- **Confidence:** high redundancy; not confirmed dead. **Status:** older audit.

### S04-DEAD-003 — Theme-specific parsing of generic animation/token concepts

- **Source:** `theme/src/lib.rs` keyframe/token parsing and component animation
  consumers under `crates/core/shell/src/shell/component/animation.rs`.
- **Current behavior:** theme keyframes and token syntax are accepted in the
  theme package, while component animation/value resolution implements separate
  registries and rules.
- **Why it matters:** accepted theme constructs can be unreachable and drift
  from component behavior.
- **Recommended improvement:** move shared syntax/lowering to the styling
  contract owner; have theme and component packages consume the same compiled
  value/keyframe representation.
- **Test:** generated projection parity for selectors, aliases, keyframes, and
  invalid syntax; classify legacy APIs before removal.
- **Confidence:** high duplication; **Status:** new architecture finding.

## Logic and core mechanics

### S04-LOGIC-001 — Theme selection bypasses the canonical installed graph

- **Source:** `crates/core/shell/src/shell/surface_layout.rs:14`, shell runtime
  theme loading, and `theme/src/lib.rs:488-562` versus graph contributions.
- **Current behavior:** a settings/theme string is joined to a filesystem theme
  directory and parsed independently of enabled, compatible graph contributions.
- **Why it matters:** uninstalled or disabled themes can render; module
  ownership, dependency closure, provenance, and profile selection are not the
  same as what the graph resolved.
- **Recommended improvement:** select a typed graph identity and mode, open only
  the graph-owned source, and publish a candidate snapshot before commit.
- **Test:** disabled/uninstalled/duplicate themes, profile selection, graph
  replacement, and source containment fixtures.
- **Confidence:** high. **Status:** older audit/backlog overlap.

### S04-LOGIC-002 — Theme identifiers are used as paths without a single safe-open boundary

- **Source:** `theme/src/lib.rs:488-500` and shell `set_theme`/watcher callers.
- **Current behavior:** caller-provided selection text participates in path
  construction; absolute, parent, and symlink cases are not all constrained by
  the graph-owned path boundary.
- **Why it matters:** settings or a service request can select content outside
  the intended module root and race a path check with replacement.
- **Recommended improvement:** never accept a path as theme identity; resolve a
  catalog record and use symlink-safe containment plus size/UTF-8 validation.
- **Test:** absolute/parent/symlink/path-swap fixtures and malicious settings
  values must fail closed without changing the active snapshot.
- **Confidence:** high. **Status:** older audit/backlog overlap.

### S04-LOGIC-003 — Reload failure is not consistently last-known-good and transactional

- **Source:** `crates/core/shell/src/shell/runtime/theme.rs:124-170`, runtime
  error propagation, and profile switching.
- **Current behavior:** malformed or half-written CSS can propagate as a shell
  error; selection/reload mutates parts of state before all consumers and
  service publication succeed.
- **Why it matters:** a theme edit can terminate or partially invalidate the
  shell, violating failure isolation and leaving observers out of sync.
- **Recommended improvement:** parse/compose/validate a candidate revision,
  publish it atomically, retain the prior revision on failure, and isolate
  subscriber errors as diagnostics.
- **Test:** invalid edit, valid recovery, callback failure, concurrent profile
  switch, and shutdown/reload race.
- **Confidence:** high. **Status:** older audit/backlog overlap.

### S04-LOGIC-004 — Modes, contributions, and sparse user overrides are not one cascade

- **Source:** `theme/src/lib.rs:198,391`, settings theme model, graph theme
  metadata, and `docs/spec/04-styling.md`.
- **Current behavior:** the graph can describe modes/contributions, but runtime
  selection and composition primarily use the direct theme file; user token
  presence and explicit empty values are not represented as a fully sparse
  layer.
- **Why it matters:** the effective winner and provenance can differ from the
  specified `pack/mode -> module contribution -> user override` cascade.
- **Recommended improvement:** compose typed layers with absent versus explicit
  empty preserved, validate cycles and selectors, and record provenance per
  effective value.
- **Test:** modes, module-scoped contributions, sparse overrides, explicit
  clears, fallback, and same-value revision tests.
- **Confidence:** high. **Status:** older audit/backlog overlap.

### S04-LOGIC-005 — Rendered theme state can disagree with service state

- **Source:** shell runtime theme publication and theme service state, plus
  `theme/src/lib.rs` revision/selection types.
- **Current behavior:** reloads, mode/color-scheme inference, and provider state
  can publish different facts than the actual rendered snapshot; name heuristics
  are used for dark/light in some paths.
- **Why it matters:** components, settings UI, accessibility, and automation can
  observe a theme that is not the one being painted.
- **Recommended improvement:** publish the committed rendered snapshot's theme,
  mode, color scheme, contrast, revision, and provenance as the authoritative
  service state.
- **Test:** same-ID reload, arbitrary mode names, provider replacement, failed
  publish, and observer ordering.
- **Confidence:** medium-high. **Status:** older audit/backlog overlap.

## Existing backlog or audit overlap

The August theme audit already covers graph bypass, path containment, mode and
cascade integration, transactional reload, durable selection, parser semantics,
keyframes, and observable state. Those are retained as overlap rather than new
backlog items. New or freshly measured candidates here are the repeated
composition/parse work and the raw/compiled/projection ownership cleanup.

## Refuted suspicions

- No speedup is claimed from broad cache retention, traversal fusion, or other
  experiments rejected in `performance-log.md`; all performance entries require
  fresh measurements.
- Missing `docs/spec/04-themes.md` is not reported as a code defect: the
  authoritative styling document is `docs/spec/04-styling.md`.

## Tests and benchmarks needed

- Canonical graph-derived theme selection, safe paths, modes, sparse cascade,
  provenance, duplicate identity, and last-known-good reload tests.
- Shared CSS/value/keyframe parser parity tests across themes and components.
- Reload and style benchmarks with explicit tree/token/file sizes, repeated
  release runs, allocations, parse passes, nodes invalidated, damage, frame
  gaps, and recovery correctness.

## File coverage

**Assigned:** 18/18: all 11 files under `crates/core/foundation/theme/`, seven
`modules/themes/**` manifest/CSS files, and `docs/spec/04-styling.md`.
**Inspected:** 18/18. Shell/UI/graph callers were searched but remain assigned
to their owning sections. **Files still needing review:** none.
