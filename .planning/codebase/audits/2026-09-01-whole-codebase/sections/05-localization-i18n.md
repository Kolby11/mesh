# Section 05 — Localization / i18n

## Scope and coverage

Reviewed all 11 assigned files: `mesh-core-locale`, its tests, the shipped
frontend i18n JSON catalogs, and `docs/spec/07-i18n.md`. Shell, component,
Luau, graph, settings, CLI, and LSP consumers were searched. **11/11 assigned
files inspected; no follow-up remains.**

## Process tree

```text
locale settings / graph catalog sources / language packs
  -> normalize selection and build fallback chain
  -> bounded parse and per-entry validation
  -> graph-authorized layered LocaleSnapshot
  -> module/core translator and Luau/template lookup
  -> diagnostics, direction, accessibility text, formatting
  -> revision propagation to mounted components and shell state
  -> reload/profile switch/recovery and durable selection
```

The current crate has a useful immutable snapshot and entry-by-entry compiler;
the remaining review focus is whether all consumers use that snapshot and
whether expensive effective-map projections are performed only when needed.

## Performance findings

### S05-PERF-001 — Effective translation projections rescan and clone complete catalogs

- **Source:** `crates/core/foundation/locale/src/lib.rs:934-979`.
- **Current behavior:** `effective_core_translations` clones strings for every
  key in the fallback chain; module projections first rebuild a `BTreeSet` of
  all keys and then resolve each key through every fallback/layer.
- **Why it matters:** debug/settings projections and script setup can turn a
  small locale change into O(catalog keys × fallback/layers) work.
- **Recommended improvement:** precompute immutable effective indexes per
  snapshot and expose lazy translators for callers that need only individual
  keys; invalidate by locale/catalog revision.
- **Measurement:** 100/10,000/100,000 keys, 2/5/10 fallback locales, 1/5/20
  layers; measure allocations, key traversals, p50/p95 projection time and RSS.
- **Confidence:** confirmed work; impact unmeasured. **Status:** new.

### S05-PERF-002 — Translator construction allocates an owned module identifier

- **Source:** `LocaleSnapshot::module_translator` at `lib.rs:775-780`.
- **Current behavior:** each translator construction copies `module_id` into a
  `String`, even though the immutable snapshot and callers commonly already
  own stable module identifiers.
- **Why it matters:** mounting/reloading many components creates avoidable
  allocation churn.
- **Recommended improvement:** use an interned/catalog-key handle or a borrowed
  lifetime where safe; benchmark before changing ownership semantics.
- **Measurement:** mount/rebuild 1/100/1,000 components with 10/100 keys;
  count allocations and total mount time, including lifetime/retention cost.
- **Confidence:** confirmed allocation, low-impact hypothesis. **Status:** new.

### S05-PERF-003 — Catalog source parsing should be measured against reload fan-out

- **Source:** `compile_catalog` and source loading at `lib.rs:1006-1054,
  1056-1092`, shell snapshot preparation callers.
- **Current behavior:** sources are parsed into JSON and then compiled into
  typed entries, with sorted entry vectors and diagnostics.
- **Why it matters:** this is intentionally safe and bounded, but repeated
  profile/reload preparation may reread unchanged catalogs.
- **Recommended improvement:** retain source fingerprints and compiled entries
  across unchanged revisions, while preserving last-known-good behavior.
- **Measurement:** 4/40/400 catalogs with unchanged versus one-file edits;
  measure bytes read, JSON allocations, compile time, peak RSS, and stale-cache
  recovery. No speedup is claimed without the workload.
- **Confidence:** medium hypothesis. **Status:** new.

## Dead code and redundancy

### S05-DEAD-001 — Legacy `TranslationSet` and the compiled catalog are parallel models

- **Source:** `lib.rs:159-163` versus `CompiledCatalog`/`CatalogEntry` at
  `:211-220` and compiler code.
- **Current behavior:** the legacy flat `HashMap<String,String>` model cannot
  represent plural/select entries, while the compiled model can; repository
  callers and fixtures must be checked before removal.
- **Why it matters:** retaining both encourages callers to bypass validation or
  silently discard structured messages.
- **Recommended improvement:** make `CompiledCatalog` the only runtime model;
  keep `TranslationSet` only as an explicit compatibility/test adapter until
  all call sites are migrated.
- **Test:** repository-wide search, compile-time deprecation, plural/select
  fixtures, and malformed-entry diagnostics.
- **Confidence:** high redundancy; not confirmed dead. **Status:** related to
  older Section 05 audit.

### S05-DEAD-002 — Core and module translation projections duplicate precedence traversal

- **Source:** `effective_core_translations` and
  `effective_module_translations` at `lib.rs:934-967`, plus translator methods
  at `:786-867`.
- **Current behavior:** point lookups and whole-map projections each implement
  fallback/layer traversal and default handling.
- **Why it matters:** precedence fixes can land in one path and not another,
  causing UI/debug/manifest text disagreement.
- **Recommended improvement:** centralize resolution in a typed effective-entry
  iterator/index and have both point and bulk APIs consume it.
- **Confidence:** confirmed duplication; **Status:** new.

## Logic and core mechanics

### S05-LOGIC-001 — Every consumer must remain module-scoped

- **Source:** `LocaleSnapshot::module_translator` and `module_entry` at
  `lib.rs:775-890`, plus shell/component/Luau callers.
- **Current behavior:** the snapshot now has separate core and module maps, but
  any caller using core translation for module-owned keys loses ownership and
  any legacy engine/global adapter can reintroduce cross-module fallback.
- **Why it matters:** translations from one module can spoof another module's
  visible text and make output order-dependent.
- **Recommended improvement:** require an explicit domain/owner in all APIs;
  remove implicit global fallback after repository-wide consumer migration.
- **Test:** duplicate keys in two modules, template/script/manifest/settings
  lookup, and unauthorized cross-module access.
- **Confidence:** medium-high seam; current snapshot isolation is present.
  **Status:** older audit/backlog overlap.

### S05-LOGIC-002 — Locale selection, catalog revision, and durable settings need one commit

- **Source:** `LocaleSelection:71-115`, `LocaleSnapshot:719-772`, shell
  locale mutation and profile/catalog preparation callers.
- **Current behavior:** locale selection and catalog snapshots carry separate
  revisions; callers can change the active locale while retaining the same
  catalog pointer, or prepare catalogs while another settings/profile revision
  is committed.
- **Why it matters:** components can render one locale while diagnostics/service
  state reports another, especially during profile switching and reload.
- **Recommended improvement:** commit a single generation containing selection,
  authorized catalogs, direction, and provenance; reject stale plans and keep
  the prior snapshot on failure.
- **Test:** concurrent locale setting, profile switch, catalog edit, invalid
  reload, and observer ordering.
- **Confidence:** medium-high; exact races depend on shell coordinator paths.
  **Status:** older audit/backlog overlap.

### S05-LOGIC-003 — Language-pack precedence and target ownership must be explicit

- **Source:** `LocaleCatalogSnapshot.modules` at `lib.rs:705-711`,
  `module_entry` at `:870-890`, graph i18n contributions, and
  `docs/spec/07-i18n.md`.
- **Current behavior:** layers can distinguish language-pack source, but the
  effective order/target and conflict diagnostics are supplied by preparation
  callers rather than encoded as a complete typed selection contract.
- **Why it matters:** pack order, module defaults, user corrections, and bundled
  catalogs can resolve differently across callers.
- **Recommended improvement:** store target module, ordered source rank, and
  provenance in the resolved snapshot; reject ambiguous identities before
  activation.
- **Test:** two packs overriding one key, module default fallback, explicit
  user correction, disabled pack, and source reporting.
- **Confidence:** medium; verify graph preparation before treating as defect.
  **Status:** older audit/backlog overlap.

### S05-LOGIC-004 — Formatting semantics remain narrower than the locale contract

- **Source:** `CatalogEntry::render` and `resolve` at `lib.rs:172-207`,
  `message_placeholders` at `:1168+`, and runtime Luau locale API consumers.
- **Current behavior:** plural/select variants are supported, but selection keys
  use conventional argument names and interpolation is string-based; number,
  date, and locale-aware formatting policy is not a uniform typed service.
- **Why it matters:** accessibility and user-facing text can differ between
  template, script, and tooling paths, and malformed arguments fail late.
- **Recommended improvement:** expose one typed formatter with explicit plural,
  select, number/date, and missing-argument diagnostics to all consumers.
- **Test:** CLDR categories, nested/select arguments, malformed placeholders,
  bidi text, and parity across template/Luau/LSP output.
- **Confidence:** medium. **Status:** older audit; do not report target-only
  formatter features as current bugs.

## Existing backlog or audit overlap

The older localization audit covers global-pool isolation, locale fallback,
plural handling, catalog lifecycle, language-pack ownership, durable locale
writes, capability/API parity, and source diagnostics. Current code has added
normalized `LocaleSelection`, bounded safe source reads, entry-level compiling,
and immutable snapshots; those completed changes are not repeated as defects.
The new findings are bulk-projection cost and duplicated resolution traversal.

## Refuted suspicions

- The current catalog compiler accepts plural/select entries and skips invalid
  siblings with diagnostics (`lib.rs:1006-1054,1095-1158`); the older “one plural
  value rejects the entire catalog” claim is refuted.
- `CatalogSourceHandle` uses relative-path checks and Unix `openat` with
  `O_NOFOLLOW` (`lib.rs:283-348,367-442`); broad path traversal is not repeated.
- No rejected performance experiment is repeated; each performance item has a
  stated workload and measurement plan.

## Tests and benchmarks needed

- Module-isolation, fallback-chain, language-pack precedence, locale revision,
  source provenance, plural/select, and last-known-good reload matrices.
- Projection/lookup parity tests so bulk and point APIs share precedence.
- Benchmarks with catalog counts/key counts/layer counts and repeated release
  runs measuring bytes read, allocations, compile/resolve time, and stale-plan
  rejection.

## File coverage

**Assigned:** 11/11: all files under `crates/core/foundation/locale/`, the
shipped frontend i18n JSON catalogs assigned to this package, and
`docs/spec/07-i18n.md`. **Inspected:** 11/11. Consumer callers were searched
but remain assigned to their owning sections. **Files still needing review:**
none.
