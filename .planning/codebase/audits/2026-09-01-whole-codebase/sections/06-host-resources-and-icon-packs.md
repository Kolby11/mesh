# Section 06 — Host resources and icon packs

## Scope and coverage

Reviewed all 18 assigned files: `mesh-core-resources`, `mesh-core-icon`, icon
and font-pack manifests/assets metadata, resource-related specs, and tests.
Shell graph/profile, renderer, XDG, font, CLI, and LSP consumers were searched.
Binary font bytes were excluded from source inspection per the coverage rules;
their manifests and mappings were inspected. **18/18 assigned files inspected.**

## Process tree

```text
host/XDG resources + canonical pack manifests + profile/settings chains
  -> validate identities, relative assets, coverage and ownership
  -> prepare graph/profile resource catalog
  -> atomically publish icon/font registry revision
  -> semantic name -> scoped pack/alias -> glyph/file target
  -> renderer/font shaping caches keyed by resource revision
  -> replacement, unload, missing-resource diagnostics, recovery
```

The current implementation has added an immutable graph/profile catalog and
resource revisions, but compatibility wrappers and host fallback paths remain
important seams to audit.

## Performance findings

### S06-PERF-001 — Uncached icon resolution can traverse every fallback layer

- **Source:** `crates/core/ui/icon/src/registry.rs:392-475,518-755`.
- **Current behavior:** a miss or a resource-revision change walks pack chains,
  semantic fallback names, aliases, XDG themes, and font glyph mappings before
  recording the result.
- **Why it matters:** repeated missing icons or large semantic chains can add
  filesystem and map work during layout/paint.
- **Recommended improvement:** retain bounded positive/negative results keyed by
  module, semantic name, size, chain identity, and resource revision; measure
  before widening caches.
- **Measurement:** 100/1,000/10,000 icon lookups with 1/5/20 packs and 0/50%
  misses; measure p95 latency, filesystem calls, allocations, and cache bytes.
- **Confidence:** confirmed traversal; impact hypothesis. **Status:** new.

### S06-PERF-002 — Codepoint-map parsing and font validation repeat across sources

- **Source:** `icon/src/xdg.rs:87-119,219-379`.
- **Current behavior:** bounded map reads and freshness checks occur on source
  changes, with parsing and validation separate from registry preparation.
- **Why it matters:** large glyph maps and repeated profile reloads can reread
  unchanged source data.
- **Recommended improvement:** share prepared immutable codepoint/font records
  through the resource snapshot; invalidate only on source fingerprint change.
- **Measurement:** 1/10/100k-entry maps and 1/10/100 fonts, cold/warm/reload;
  record bytes read, parse CPU, peak RSS, and stale-source correctness.
- **Confidence:** medium hypothesis; **Status:** new.

### S06-PERF-003 — Resource replacement invalidates broader caches than necessary

- **Source:** `icon/src/registry.rs:378-475` and renderer resource consumers.
- **Current behavior:** registry generation/resource revision changes invalidate
  resolution state at registry scope, while a single pack or glyph-map change
  may affect only a subset of module/name bindings.
- **Why it matters:** profile edits can trigger repeated lookups and repaints for
  unrelated icons.
- **Recommended improvement:** include scoped pack/asset revisions in cache keys
  and propagate changed-resource sets to consumers.
- **Measurement:** one-pack versus all-pack replacement in a 1,000-node tree;
  measure invalidated entries, lookup CPU, paint damage, and frame gap.
- **Confidence:** medium. **Status:** new measurement hypothesis.

## Dead code and redundancy

### S06-DEAD-001 — Process-global icon convenience APIs duplicate snapshot ownership

- **Source:** `crates/core/ui/icon/src/lib.rs:38-175` versus
  `IconRegistry::from_catalog/replace_bindings` at `registry.rs:240-378`.
- **Current behavior:** global default-registry setters and the explicit
  graph/catalog registry both expose mutation and resolution paths.
- **Why it matters:** callers can bypass profile ownership and use stale global
  state; removal is unsafe until all dynamic/plugin callers are audited.
- **Recommended improvement:** make snapshot handles the runtime API and retain
  global functions only as a clearly scoped compatibility adapter for tests or
  legacy host integration.
- **Test:** repository-wide call graph and migration tests; verify no shell or
  renderer path uses the global adapter.
- **Confidence:** high parallel authority, not confirmed dead. **Status:** older
  audit/backlog overlap.

### S06-DEAD-002 — Legacy fallback vocabulary and canonical pack chains overlap

- **Source:** `icon/src/fallback.rs` and legacy config/discovery callers versus
  `bindings.rs:108-145` and `registry.rs` chain resolution.
- **Current behavior:** built-in semantic fallbacks coexist with canonical
  manifest/profile chain resolution.
- **Why it matters:** authors cannot tell which vocabulary owns a fallback and
  old fixtures can preserve behavior that canonical modules no longer declare.
- **Recommended improvement:** make fallback policy one typed chain stage,
  document it, then remove the compatibility table after consumer review.
- **Confidence:** medium; classify each symbol via repository-wide search.
  **Status:** older audit.

## Logic and core mechanics

### S06-LOGIC-001 — Resource authorization must be committed with graph/profile generation

- **Source:** `resources/src/lib.rs`, `icon/src/registry.rs:240-378`, and shell
  discovery/profile/resource consumers.
- **Current behavior:** the explicit catalog/registry supports atomic replacement,
  but convenience/default APIs and host fallback discovery remain available
  outside the graph/profile candidate boundary.
- **Why it matters:** disabled, removed, or superseded packs can remain
  resolvable, or rendering can observe a resource revision different from the
  active profile.
- **Recommended improvement:** prepare one graph-authorized resource snapshot,
  publish it with activation, and make all consumers use that handle.
- **Test:** profile switch, uninstall, failed candidate, pack replacement, and
  stale lookup after generation change.
- **Confidence:** medium-high seam; **Status:** older audit/backlog overlap.

### S06-LOGIC-002 — Asset paths need a single no-follow containment boundary

- **Source:** `icon/src/xdg.rs:197+`, pack registration callers, and renderer
  icon loading.
- **Current behavior:** some canonical source handles are validated, but asset
  joins and late render-time reads still need one uniform module-root policy.
- **Why it matters:** parent/absolute paths, symlink swaps, and external SVG
  references can cross a module boundary or make a prepared snapshot stale.
- **Recommended improvement:** resolve and validate regular assets during
  preparation, retain verified handles/digests, and separate explicitly trusted
  user overrides from untrusted module assets.
- **Test:** traversal, symlink, race, external-reference, missing-file, and
  rollback fixtures.
- **Confidence:** medium-high; verify each current caller before implementation.
  **Status:** older audit/backlog overlap.

### S06-LOGIC-003 — Alias ownership and pack order must remain deterministic

- **Source:** `bindings.rs:108-177` and registry resolution at
  `registry.rs:636-755`.
- **Current behavior:** chains are ordered and validation exists, but aliases,
  semantic fallback, system packs, and qualified targets cross multiple maps and
  fallback stages.
- **Why it matters:** duplicate aliases or same-name packs can yield different
  icons by iteration/registration order, and an alias may resolve from the
  wrong owner.
- **Recommended improvement:** compile an ordered bidirectional ownership index,
  reject duplicates before publication, and return source provenance for every
  result.
- **Test:** duplicate pack IDs/aliases, owner replacement, chain reordering,
  module-scoped vocabulary, and XDG fallback.
- **Confidence:** medium-high. **Status:** older audit; current tests cover some
  cases but not all cross-source combinations.

### S06-LOGIC-004 — Font-pack declarations and text-font resolution remain separate

- **Source:** resources `font.rs`, icon font/glyph code, and UI text shaping
  consumers; canonical font-pack manifests.
- **Current behavior:** icon glyph assets can resolve through pack bindings, but
  ordinary text font selection follows the shaping system's family lookup rather
  than one graph-authorized font-pack chain.
- **Why it matters:** installing/selecting a font pack may not affect text
  shaping or may be reported available while unused.
- **Recommended improvement:** define whether font packs own glyph assets,
  text families, or both; publish typed family/asset capabilities and provenance
  in the resource snapshot.
- **Test:** font-pack install/uninstall, fallback family, shaping revision, and
  unavailable-font diagnostics.
- **Confidence:** medium; contract boundary needs explicit product decision.
  **Status:** older audit.

## Existing backlog or audit overlap

The August resources audit covers graph bypass, path safety, pack order/aliases,
legacy fallbacks, multicolor/coverage semantics, cache freshness, font-pack
integration, host discovery, blocking work, and diagnostics. Current code has
added explicit catalog handles, scoped mappings, bounded reads, and resource
revisions; these improvements are not restated as current defects. New items
are primarily scoped-resolution and workload measurements.

## Refuted suspicions

- Registry tests now cover owner-scoped mappings, duplicate rejection, chain
  ordering, and revision-aware negative-cache retry (`registry.rs:949+`); a
  blanket nondeterminism claim is not promoted without a missing-case fixture.
- Bounded glyph-map/font reads and cancellation exist (`xdg.rs:338-379`);
  unbounded input is not reported.
- No rejected cache or layout experiment is repeated without new measurements.

## Tests and benchmarks needed

- Resource snapshot atomicity, pack/alias order, module ownership, safe asset
  opening, font-pack semantics, XDG fallback, diagnostics, and rollback.
- Lookup benchmarks with pack/alias/miss counts and explicit resource revisions;
  record CPU, allocations, filesystem reads, cache size, and paint damage.

## File coverage

**Assigned:** 18/18 under `crates/core/foundation/resources/` and
`crates/core/ui/icon/`, plus canonical icon/font-pack manifests and relevant
resource specs/fixtures. **Inspected:** 18/18; binary font bytes excluded as
documented in `00-coverage.md`. **Files still needing review:** none.
