# Section 5 — Localization / i18n: improvement audit

**Audited:** 2026-08-19
**Scope:** `mesh-core-locale`, canonical i18n contributions and diagnostics,
shell startup/profile/reload paths, component and Luau consumption, localized
manifest/props data, settings persistence, capabilities, and CLI/LSP tooling.

This is a point-in-time review record, not a second task tracker. Open work from
this audit lives in [`docs/BACKLOG.md`](../../../../docs/BACKLOG.md).

## Logical process map

The shipped path currently has three localization authorities which never form
one coherent snapshot:

```text
settings.json + optional active profile       canonical InstalledModuleGraph
  shell.i18n.locale/fallback_locale                     │
                 │                                      ▼
                 ▼                              contributed_i18n()
      Shell LocaleEngine                        {owner, locale, path}
      (selection, no catalogs)                           │
                 │                                      ▼
                 │                           raw module_dir.join(path)
                 │                                      │
                 │                     repeated for every mounted surface
                 │                                      ▼
                 └──────── locale string ──► FrontendSurfaceComponent
                                               ├─ private LocaleEngine
                                               ├─ reread/parse every catalog
                                               ├─ module maps + global pool
                                               └─ copied map per ScriptContext
                                                        │
                       ┌────────────────────────────────┼───────────────┐
                       ▼                                ▼               ▼
               template t(expr)                 mesh.i18n.t()   manifest text
               global lookup                    module map      mixed resolvers
                       │                                │               │
                       └──────── miss: raw key ─────────┴──── diagnostics vary

mesh.locale.set(locale)
  └─ shell event → prepend locale to old chain → refresh all surfaces
       ├─ does not persist settings
       ├─ does not carry the configured/derived chain to components
       └─ publishes only { current, locale }

catalog edit / pack enablement / retained surface during profile switch
  ╳ no catalog watcher, shared candidate, atomic commit, or live refresh
```

The result is that graph state controls which paths are handed to a newly
mounted surface, each surface independently owns catalog contents, and the
shell owns locale selection and some manifest lookups without owning any
catalog contents.

## Confirmed findings

### 1. Critical — module-scoped translations leak through a global pool

`LocaleEngine::load_module_translations` stores a catalog under its module ID
and then also merges it into `translations`
(`crates/core/foundation/locale/src/lib.rs:70`). Both
`translate_for_module` and `effective_translations_for_module` fall back to
that pool (`:94` and `:109`). The last loaded module silently wins duplicate
global keys.

Template evaluation is less safe still: `LocaleBoundState` has no module ID and
calls global `translate` (`crates/core/runtime/scripting/src/context/state.rs:412`).
A key missing from one module can therefore render text owned by an unrelated
module, with output depending on graph/catalog load order. This violates the
specification's primary isolation rule and lets an untrusted module spoof text
inside another module.

**Improve it:** remove implicit global fallback. Every lookup must carry an
owning module or an explicitly named interface/core catalog domain. Build a
module-scoped translator from deterministic layers and use it for templates,
scripts, manifests, props, debug data, and generated settings UI.

### 2. Critical — locale selection and fallback semantics are incorrect

`set_locale` inserts a new string at the front of the existing fallback vector
and deduplicates it (`crates/core/foundation/locale/src/lib.rs:56`). Old active
locales remain as accidental fallbacks. Constructors create only an exact
active/fallback pair (`:23` and `:33`): there is no BCP 47 validation,
canonicalization, or region/script parent derivation, so `sk-SK` does not find
a `sk` catalog.

Components receive only `locale.current()` and mutate their own accumulated
chain (`crates/core/shell/src/shell/component/shell_component/mod.rs:1117`).
Changing `fallback_locale` can consequently leave component chains different
from the shell. Module `mesh.i18n.defaultLocale` is not carried in graph catalog
records and cannot serve as the required terminal per-module fallback
(`shell/discovery.rs:859`).

**Improve it:** define an immutable, normalized `LocaleSelection` containing
active tag, explicit/derived chain, direction, and revision. Replace rather
than mutate the chain, append each module's declared default only for that
module, and deliver the complete selection to every consumer.

### 3. Critical — a plural value rejects its entire otherwise-valid catalog

`TranslationSet.messages` is `HashMap<String, String>`
(`crates/core/foundation/locale/src/lib.rs:7`), and surface mounting parses the
whole JSON file into the same type
(`crates/core/shell/src/shell/component/runtime.rs:248`). A specification-valid
`_plural` object makes deserialization fail and the loader skips every string
in that file. The only interpolation implementation is an unused, global-only
`translate_with`; Luau exposes only `t(key)`
(`crates/core/runtime/scripting/src/context/runtime/helpers.rs:125`).

**Improve it:** compile bounded typed catalog entries independently: messages,
CLDR plural/select variants, and checked placeholders. Invalid entries should
produce source-located diagnostics without discarding valid siblings. Expose
one module-aware interpolation/plural/number/date/duration formatter to every
consumption surface.

### 4. High — catalog lifecycle is stale, repeated, and non-transactional

Every surface synchronously reads and parses every graph catalog during mount
(`crates/core/shell/src/shell/component/shell_component/mod.rs:79` and
`shell/component/runtime.rs:248`). Existing surfaces retain those private maps.
The watcher includes settings, theme, and component source paths but no catalog
files (`crates/core/shell/src/shell/runtime/mod.rs:443`). Graph changes and
profile switches attach a new path vector only to newly constructed surfaces;
retained surfaces keep the old set (`shell/profile.rs:520` and `:855`).

Settings/profile locale changes reconstruct only the shell engine, which has no
catalogs, then ask each component to reinterpret its existing maps
(`shell/runtime/theme.rs:395` and `shell/profile.rs:801`). An unreadable or
malformed catalog is only a warning and a candidate can commit with a partial
translation set.

**Improve it:** load, parse, validate, and fingerprint graph-authorized inputs
once into an immutable `CatalogSnapshot`. Startup, graph changes, profile
switching, selection changes, and file reload should prepare one complete
candidate, atomically commit its generation, and retain the last-known-good
snapshot on failure. Rebind catalog watches after every committed graph.

### 5. High — language-pack targeting, order, and provenance are absent

The specification marks language-pack layering as target, but the current data
model cannot express it. `I18nContribution` contains only `id`, `locale`, and
`path` (`crates/core/extension/module/src/package/module_manifest.rs:1353`).
Graph indexing assigns the source module as the translation owner
(`installed_graph/contributions.rs:193`); there is no target module, active pack
chain, key-by-key precedence, or provenance.

**Improve it:** add a validated target domain and deterministic ordered pack
selection to the canonical graph/profile model. Compose user correction,
ordered packs, module-bundled locale, user fallback locales, and module default
exactly once, retaining the winning source for `which`, diagnostics, and debug
inspection. Detect duplicate identities and ambiguous precedence before
activation.

### 6. High — locale writes are non-durable and module defaults corrupt live state

`apply_set_locale` trims only empty input, mutates the in-memory engine, and
broadcasts (`crates/core/shell/src/shell/runtime/theme.rs:455`). It does not
write the sparse settings/profile store, so restart or settings reload loses
the choice.

Separately, component settings interpret `i18n.default_locale` as a new active
locale (`shell/component/shell_component/mod.rs:1226`). That path neither
refreshes the copied Luau translation maps nor runs the normal locale
invalidation flow, so a component engine, existing `t()` closure, and the shell
can disagree.

**Improve it:** route manual, service, settings, and profile changes through one
revision-checked durable selection transaction. Treat module default locale as
metadata in resolution, never as a component-local active-locale override.

### 7. High — the Luau API and capability boundary contradict themselves

Runtime documentation and LSP knowledge advertise
`mesh.locale.translate(key, params)`, but `install_locale_api` installs only
`current` and `set` (`crates/core/runtime/scripting/src/host_api.rs:14` and
`context/runtime/host_api.rs:125`). A documented call therefore resolves to
`nil`.

The direct host table is returned before interface-proxy resolution.
`mesh.locale.current()` is installed without a `locale.read` check, and
`require("mesh.i18n")` returns the catalog helper before capability checks
(`context/runtime/host_api.rs:125` and `:368`). A module without `locale.read`
can inspect locale state and probe effective keys, while `set` alone checks
`locale.write`.

**Improve it:** choose one contract: `mesh.i18n` should be the scoped translator
and `mesh.locale` the typed selection/formatting interface, or combine them
deliberately. Generate runtime and LSP surfaces from the same ABI and enforce
read/write capabilities consistently at creation and every operation.

### 8. High — misses and localized metadata are handled inconsistently

Luau `t()` and template `t(expr)` return the raw key without a diagnostic
(`crates/core/runtime/scripting/src/context/runtime/helpers.rs:131` and
`crates/core/frontend/compiler/src/expr.rs:322`), rather than the specified
visible `!!key`. Manifest text has a separate resolver which can emit a degraded
diagnostic, while the shell/debug engine generally has no loaded catalogs.

Props preserve `{t, fallback}`, but generated frontends contain ad hoc
resolution logic and can fall back to a prop name instead of resolving in the
owner module. Static graph diagnostics inspect only literal `t()` calls against
one default-locale catalog (`crates/core/extension/module/src/package/installed_graph/diagnostics.rs:510`),
not all localized metadata, placeholders, pack layers, or runtime dynamic keys.

**Improve it:** centralize `LocalizedText` and miss resolution with owner,
field/key, fallback, winning source, and snapshot revision. Deduplicate active
miss diagnostics per generation and make all UI/tooling consumers use it.

### 9. Medium — catalog opening adds localization-specific security and latency risk

Contribution paths receive only lexical relative-path validation. The shell
later reconstructs a raw `module_dir.join(path)` and each surface calls
`read_to_string` (`crates/core/shell/src/shell/discovery.rs:859` and
`shell/component/runtime.rs:248`). Symlink components can escape the module
root, the target can change between graph validation and open, and there are no
byte, key-count, or value-length limits. Mount cost scales as surfaces × all
catalogs and runs synchronously on the shell thread.

**Improve it:** reuse the module system's contained, no-follow source handles;
bound input and compiled snapshot sizes; parse once off the shell thread; and
commit by fingerprint. The general filesystem-containment and blocking-I/O
backlog items already cover the shared mechanisms, so this audit does not add
duplicates for them.

### 10. Medium — authoring/tooling surfaces do not follow the canonical graph

LSP locale discovery scans `config/i18n/*.json` and legacy/default metadata
instead of canonical arbitrary `mesh.provides.i18n[].path`
(`crates/tools/lsp/src/module_registry.rs:292`). It can miss valid locales and
cannot model profile enablement, pack targets, or precedence. The locale CLI
commands in specification section 8 (`list`, `active`, `set`, `which`,
`missing`, and `extract`) are not implemented.

The component parser also accepts `<i18n>` as a known block, but `ComponentFile`
has no corresponding field and `parse_component` discards it silently
(`crates/core/ui/component/src/parser.rs:43` and `:82`).

**Improve it:** drive LSP, CLI, extraction, and doctor output from the compiled
catalog/graph model. Either implement inline component catalogs with explicit
precedence or reject `<i18n>` with a migration diagnostic; never accept and
discard author input.

## Recommended target architecture

```text
InstalledModuleGraph + profile resource selection + sparse settings
        │
        ▼
LocaleCoordinator::prepare(graph, selection, prior_snapshot)
  ├─ validate/canonicalize BCP 47 active tag and fallback chain
  ├─ open contained, bounded catalog sources
  ├─ compile typed message/plural/select/placeholder entries
  ├─ resolve target modules and ordered language-pack layers
  └─ produce Arc<CatalogSnapshot>
       { revision, selection, direction, module translators,
         source provenance, structured diagnostics }
        │
        ▼
atomic last-known-good commit + durable settings/profile revision
        │
        ├─ module-scoped Translator handles for template/Luau/manifest/props
        ├─ one LocaleChanged event with selection + snapshot revision
        ├─ dependency-aware component invalidation
        └─ CLI/LSP/doctor inspection of the same graph and provenance
```

A useful feature beyond the current flow is an explicit locale policy
(`manual`, `follow-system`, or per-profile). Portal/environment changes should
enter the same prepare/persist/commit path, and direction should be snapshot
metadata so RTL layout changes cannot race translation changes.

## Recommended implementation order

1. Add regressions for cross-module leakage, stale fallback accumulation,
   region/script fallback, plural-file rejection, non-durable selection,
   retained-surface catalog staleness, API drift, and capability bypass.
2. Freeze the canonical settings and contribution shapes; validate BCP 47,
   nonempty/unique contribution identities, target modules, and pack order.
3. Introduce typed catalog entries, per-entry diagnostics, limits, contained
   source handles, and the immutable central snapshot.
4. Remove the global pool and pass module-scoped translators through every
   template, script, manifest, props, settings, and debug surface.
5. Implement language-pack composition and per-module terminal defaults with
   provenance and deterministic precedence.
6. Route startup, settings/service writes, profile/graph changes, and catalog
   reload through one durable prepare/commit coordinator.
7. Align `mesh.i18n`, `mesh.locale`, service state/events, capabilities, LSP,
   and CLI with one generated contract; add formatting and directionality.
8. Add catalog watches, narrow invalidation by translator generation/key
   dependencies, system-locale policy, extraction, missing-key, and `which`
   tooling.

## Required regression coverage

- A module never resolves another module's catalog key, regardless of graph or
  load order; explicit interface catalogs remain independently scoped.
- BCP 47 tags normalize and derive parents (`zh-Hant-TW → zh-Hant → zh`), stale
  active locales never remain in the chain, and each module default is terminal
  only for that module.
- String and plural/select entries coexist in one file; Slovak and English
  categories, placeholder consistency, malformed sibling isolation, and size
  limits are covered.
- Two targeted packs shadow one module key in configured order; unrelated or
  disabled packs do not participate; provenance identifies the winner.
- Manual locale selection persists in the correct profile/settings scope and
  survives restart; stale revisions and invalid tags do not mutate live state.
- Catalog edits, graph enable/disable, and profile switches refresh retained and
  new surfaces on one revision; an invalid candidate retains the old snapshot.
- Catalog paths cannot escape through absolute, parent, or symlink traversal;
  catalog bytes are read and parsed once per snapshot rather than once per
  surface.
- Templates, existing Luau `t()` handles, manifests, props, generated settings,
  and debug data resolve the same module/revision and emit `!!key` plus one
  structured miss diagnostic.
- Runtime, service, documentation, and LSP APIs agree; `locale.read` and
  `locale.write` denial is enforced consistently.
- `<i18n>` is either compiled with documented precedence or rejected explicitly;
  CLI/LSP locale discovery follows canonical graph contributions.

## Verification

Four Luna xhigh review passes reconstructed the flow, challenged its logical
order and feature model, inspected concrete code defects, and audited catalog,
filesystem, capability, and tooling boundaries. No reviewer edited production
code.

Executed locally with `nix develop`:

```text
mesh-core-locale: 3 passed
mesh-core-scripting import slice: 20 passed
mesh-core-module i18n slice: 3 passed
mesh-core-shell locale slice: 7 passed, 1 failed before its localization
assertion on the already-recorded legacy mesh.surfaceLayout fixture baseline
```

The passing suites validate isolated current behavior but do not cover the
cross-boundary failures above. No production code was changed by this audit.
