# Section 07 — Component language

## Scope and coverage

Reviewed all 13 assigned files under `crates/core/ui/component/`, the component
language/specification documents, and shipped `.mesh` consumers at the compiler
seam. Compiler, host, shell runtime, style, LSP, and module graph callers were
searched. **13/13 assigned files inspected; no follow-up remains.**

## Process tree

```text
.mesh source
  -> span-preserving block/script/markup/style parse
  -> props/import/template/style semantic validation
  -> ComponentFile AST and dependency graph
  -> frontend compiler/module-root resolution
  -> Luau/runtime props and element-tree creation
  -> style/layout/render and LSP diagnostics
  -> file change/recompile/recovery
```

## Performance findings

### S07-PERF-001 — Component parsing repeats lexical scans across independent branches

- **Source:** `component/src/parser.rs:193-310,449-457`, parser/script and
  parser/markup modules.
- **Current behavior:** top-level blocks, scripts, braces, markup, props, and
  styles each rescan source and allocate intermediate strings/spans.
- **Why it matters:** editor updates and hot reload pay whole-file work even for
  a local edit and retain several representations.
- **Recommended improvement:** tokenize once with shared source spans, then
  derive block/AST views; cache unchanged branch results by source fingerprint.
- **Measurement:** 1/10/100 KiB files, 1/10/100 edits, and 10/1000 components;
  measure bytes scanned, allocations, p50/p95 parse time and stale-reuse rate.
- **Confidence:** confirmed repeated scans; impact hypothesis. **Status:** new.

### S07-PERF-002 — Dependency/style validation can repeat full subtree work

- **Source:** `parser/brace.rs:231-245`, `parser/script.rs:222-243,695+`, and
  downstream frontend compile validation.
- **Current behavior:** expression/script symbol and import analyses are run in
  separate passes after parsing.
- **Why it matters:** large components and recursive local imports amplify
  editor latency and duplicate AST traversal.
- **Recommended improvement:** retain one semantic index on `ComponentFile` and
  invalidate only changed blocks/import subgraphs.
- **Measurement:** 10/100/1,000-component graphs with one leaf edit; measure
  nodes reparsed, graph walks, allocations, and p95 compile latency.
- **Confidence:** medium. **Status:** new; no speedup claimed.

## Dead code and redundancy

### S07-DEAD-001 — Tooling and runtime parsing entry points are parallel authorities

- **Source:** `parser.rs:193-204,432-437` and `parse_component_for_tooling`.
- **Current behavior:** tooling and runtime have distinct parse paths/options,
  creating two interpretations of blocks and recovery behavior.
- **Why it matters:** LSP can accept or locate syntax that runtime rejects, and
  compatibility helpers can become permanent alternate grammar.
- **Recommended improvement:** share one lossless parser/AST with explicit
  recovery mode; keep tooling-only partial nodes as a documented adapter.
- **Test:** source corpus parity, mid-edit malformed blocks, exact spans, and
  runtime/tooling diagnostic comparison.
- **Confidence:** high parallel authority; not confirmed dead. **Status:** older
  audit/backlog overlap.

### S07-DEAD-002 — Style value/selector lowering duplicates theme CSS semantics

- **Source:** `component/src/style.rs:90-292` and `parser/styles.rs:34-168`,
  alongside `mesh-core-theme::css`.
- **Current behavior:** component package lowers selectors, declarations,
  keyframes, easing, and prop references into its own types while theme CSS has
  related syntax/lowering types.
- **Why it matters:** supported syntax and error rules can drift between theme
  and component styles.
- **Recommended improvement:** make the shared CSS/value AST canonical and
  retain only component-specific scope lowering.
- **Confidence:** confirmed duplication; **Status:** new/related Section 04.

## Logic and core mechanics

### S07-LOGIC-001 — Local imports need one module-root-safe resolver

- **Source:** `parser/script.rs:134-140` and compiler local import reads.
- **Current behavior:** authored local targets are represented as strings and
  downstream joining/canonicalization must enforce containment.
- **Why it matters:** absolute/parent paths or symlink replacement can read
  outside the owning module, violating module isolation.
- **Recommended improvement:** parse into a validated module-relative target,
  reject traversal/absolute paths, canonicalize before read, and reuse it for
  imports, entrypoints, watchers, and LSP.
- **Test:** `../`, absolute, `@src/../../`, symlink, and disappearing-file cases.
- **Confidence:** high. **Status:** older audit/backlog overlap.

### S07-LOGIC-002 — Local alias ownership is not always part of component identity

- **Source:** compiler import collection and alias insertion; component refs in
  `template.rs:327-458`.
- **Current behavior:** nested local components can be indexed by a bare alias,
  allowing branches that both import `Item` to overwrite one another.
- **Why it matters:** the element tree can instantiate a different component
  than the author selected, with traversal-order-dependent behavior.
- **Recommended improvement:** resolve `(owner component, alias)` to canonical
  target identity, detect collisions/cycles, and retain authored paths for
  diagnostics.
- **Test:** two branches with same alias/different files, cycles, and module
  component name collisions.
- **Confidence:** high. **Status:** older audit/backlog overlap.

### S07-LOGIC-003 — Parser recovery must not turn malformed expressions into literals

- **Source:** `parser/brace.rs:100-231,325-470` and template expression lowering.
- **Current behavior:** brace/interpolation recovery classifies malformed input
  through scanner fallback; some failures can survive as text or incomplete
  nodes instead of a source-located invalid expression.
- **Why it matters:** runtime can render source text, skip binding, or diverge
  from LSP diagnostics rather than fail the component candidate.
- **Recommended improvement:** preserve an invalid-expression node with span and
  diagnostic; prohibit silent literal fallback in runtime mode.
- **Test:** unmatched braces, quoted/long strings, nested tables, malformed
  Luau, and runtime/tooling parity.
- **Confidence:** medium-high. **Status:** older audit/backlog overlap.

### S07-LOGIC-004 — Props, CSS `prop()`, and Luau fields need one typed declaration

- **Source:** `parser/props.rs:272-474`, `style.rs:254-292`, and component
  compile/runtime prop publication.
- **Current behavior:** props are parsed/validated in one branch while CSS
  references and runtime/Luau publication resolve names separately.
- **Why it matters:** an undeclared CSS reference or a higher-precedence invalid
  value can be accepted, dropped, or stringify a structured value differently
  across layers.
- **Recommended improvement:** compile one typed prop schema with defaults,
  localization, CSS projection, reactive field, and settings metadata; validate
  all overrides before precedence selection.
- **Test:** missing/unknown props, structured values, invalid high-layer values,
  defaults, localized labels, and CSS/Luau/settings parity.
- **Confidence:** high. **Status:** older audit/backlog overlap.

### S07-LOGIC-005 — Source spans and semantic validation must survive all projections

- **Source:** `ComponentFile` construction in `parser.rs:206-310`, style/script
  parsers, frontend compiler and LSP consumers.
- **Current behavior:** independent lowering and string-based reanalysis can
  lose exact source ownership for imports, expressions, props, and styles.
- **Why it matters:** diagnostics, reload invalidation, accessibility metadata,
  and generated settings cannot reliably point back to the authoring source.
- **Recommended improvement:** make lossless AST/spans the package contract and
  attach semantic errors to those nodes; have compiler, runtime, and LSP consume
  the same result.
- **Test:** Unicode/CRLF, nested blocks, partial edits, duplicate names, and
  source-to-runtime diagnostic mapping.
- **Confidence:** high. **Status:** older audit/backlog overlap.

## Existing backlog or audit overlap

The prior component audit covers path containment, alias collisions, block
extraction, interpolation recovery, props/CSS joining, coercion, partial
validation, script analysis, and source locations. Current code has parser-owned
Luau AST/token work and shared CSS lowering in places; this report does not
repeat fixed claims. New observations are parse-pass and style-projection
measurement plus ownership consolidation.

## Refuted suspicions

- `parse_component_for_tooling` and parser recovery are current APIs; they are
  not called dead without a full LSP/partial-source call-graph review.
- No parser cache speedup is asserted; benchmark both cold and incremental
  edits, and do not repeat rejected experiments without new evidence.

## Tests and benchmarks needed

- Root-safe import graph, alias scope/cycles, malformed-expression, props
  precedence, style selector, Unicode span, and tooling/runtime parity tests.
- Incremental parser/compiler benchmarks with file sizes, graph sizes, edit
  shapes, allocations, AST nodes reparsed, and p95 latency.

## File coverage

**Assigned:** 13/13 under `crates/core/ui/component/` and the component syntax
specification documents. **Inspected:** 13/13. Compiler/runtime/LSP callers
were searched but belong to Sections 10, 11, and 16. **Files still needing
review:** none.
