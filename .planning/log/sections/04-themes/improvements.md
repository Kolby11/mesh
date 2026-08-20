# Section 4 — Themes: improvement audit

**Audited:** 2026-08-19  
**Scope:** `mesh-core-theme`, including canonical module/theme contributions,
profile and settings selection, shell loading and reloads, style resolution,
animation/keyframes, service state, diagnostics, and invalidation consumers.

This is a point-in-time review record, not a second task tracker. Open work from
this audit lives in [`docs/BACKLOG.md`](../../../../docs/BACKLOG.md).

## Logical process map

The shipped runtime and the canonical module graph currently follow parallel
paths which never join:

```text
settings/profile theme string                 canonical module.json
             │                                         │
             ▼                                         ▼
 theme_path_for_id(string)                    InstalledModuleGraph
             │                                 ├─ theme contribution IDs
             ▼                                 ├─ mode → relative CSS paths
 config/themes filesystem scan                 ├─ default mode + provenance
 ├─ private sibling module.json parser         └─ enablement/dependencies
 ├─ handwritten theme.css parser                         │
 └─ legacy JSON theme parser                             ╳  not consumed
             │
             ▼
 ThemeEngine { active Theme, Vec<Theme> }
             │
             ├─ global node/tag defaults
             ├─ optional in-file @module data
             ├─ token lookup
             └─ tooltip-only theme keyframes
             │
             ▼
 StyleResolver → component tree → render/present
             │
             └─ change: clear caches, full present, core/service events
```

The intended load-time cascade also has three inputs which are absent from that
live path: the selected mode of the active theme pack, enabled frontend
`mesh.theme` contributions, and sparse user token overrides.

## Confirmed findings

### 1. Critical — live theme loading bypasses the canonical module graph

The installed graph indexes canonical `mesh.provides.themes` records, mode
paths, default modes, module ownership, and enablement
(`crates/core/extension/module/src/package/installed_graph/contributions.rs:168`
and `graph.rs:586`). The shell never consumes that catalog. Startup instead
loads the settings string through `theme_path_for_id` and later scans
`theme_dir_path()` directly (`crates/core/shell/src/shell/surface_layout.rs:14`
and `shell/runtime/mod.rs:198`).

The direct loader has its own manifest schema: it expects `mesh.theme.id` and
`mesh.theme.label` beside `theme.css`
(`crates/core/foundation/theme/src/lib.rs:562`). Canonical validation permits
`mesh.theme` only for frontend modules and requires theme modules to use
`mesh.provides.themes`
(`crates/core/extension/module/src/package/module_manifest.rs:462` and `:485`).
The shipped `config/themes/*/module.json` files consequently belong to a
parallel format which the canonical theme-module path rejects.

This makes filesystem presence, rather than the enabled compatible graph, the
authority for availability and activation. Install/uninstall state, profile
closure, provenance, duplicate diagnostics, and mode selection cannot reliably
control what the shell renders.

**Improve it:** make a graph-derived `ThemePackDescriptor` catalog the only
theme authority. A descriptor should carry a scoped identity, owner module,
localized label, ordered modes, validated default mode, and contained source
handle. Move shipped themes into the ordinary installed inventory and remove
the private metadata parser, legacy JSON path, and direct `config/themes` scan.

### 2. Critical — theme IDs can escape the theme/package boundary

`theme_path_for_id` joins the settings or service-supplied string directly onto
the theme directory (`crates/core/foundation/theme/src/lib.rs:488`). Absolute
paths replace the base and `..` components escape it. `set_theme` accepts any
nonblank string and can pass it to this loader
(`crates/core/shell/src/shell/runtime/request.rs:1907` and
`shell/runtime/theme.rs:165`). Direct discovery also follows directory
symlinks. A settings edit or theme-control caller can therefore make the shell
read uninstalled CSS/JSON and an adjacent manifest outside the intended root.

Canonical contribution validation rejects lexical absolute/parent paths, but
does not itself prove canonical containment, reject a discovered symlink, or
protect the open against a path swap.

**Improve it:** selection must resolve only a catalog identity, never construct
a path from caller text. Open the selected mode beneath its owning module root
with symlink-safe, race-resistant containment; validate file type, size, UTF-8,
and content before it enters a candidate snapshot.

### 3. Critical — invalid reloads and profile changes are not last-known-good transactions

A malformed hot edit is converted to `ShellRunError::Theme` with `?`
(`crates/core/shell/src/shell/runtime/theme.rs:124`), and the main run loop also
uses `?` (`shell/runtime/mod.rs:249`). A half-written `theme.css` can terminate
the whole shell instead of preserving the last valid theme and reporting a
recoverable diagnostic.

Successful selection/reload mutates the active engine before fallible component
callbacks and service publication. One callback error can leave only some
components invalidated. Profile switching similarly commits other candidate
state before loading the requested theme and treats a missing/invalid selection
as a fallback rather than a failed candidate
(`crates/core/shell/src/shell/profile.rs:783`).

**Improve it:** parse, compose, validate, and prepare an immutable candidate
snapshot before any visible mutation. Commit one snapshot generation
infallibly; report subscriber failures separately. Reload failure retains the
old snapshot and records a keyed diagnostic, while a later valid edit can
recover. Profile switching must include the exact theme/mode snapshot in its
prepare/commit boundary.

### 4. High — modes and the load-time cascade do not exist in the runtime

The styling specification describes mode-capable theme packs and the cascade
`pack/mode → module contributions → user token overrides`. The graph stores
`modes` and `default_mode`, but `Theme`, `ThemeEngine`, and `ThemeSettings` have
no selected-mode model; settings only contain `active`
(`crates/core/foundation/theme/src/lib.rs:198` and `:391`;
`crates/core/foundation/config/src/lib.rs:233`). The service exposes no
`modes()`, `active_mode()`, `set_mode()`, token overrides, or token provenance.

The resolver can apply `Theme.modules`, but production only populates it from a
theme file's private `@module` blocks. Enabled frontend manifest `mesh.theme`
contributions are never composed into the active theme. User `theme.tokens`
overrides are rejected by the current settings schema. A global theme pack can
therefore name another module in `@module`, while the module-owned contribution
contract is inert—the reverse of the intended ownership boundary.

**Improve it:** add a pure composer over four explicit layers:

```text
base recovery defaults
→ selected graph theme pack + mode
→ each enabled frontend's contribution, scoped to its own module ID
→ sparse profile/user token overrides
```

The output should record per-value provenance and validate selectors,
properties, token references/cycles, keyframes, and unresolved values before
commit. Mode metadata should carry color-scheme and contrast semantics rather
than relying on naming conventions.

### 5. High — `set_theme` is non-durable and can activate stale content

`apply_set_theme` changes only `self.settings.theme.active`
(`crates/core/shell/src/shell/runtime/theme.rs:161`). Unlike the icon and font
setters, it neither updates/saves `SettingsStore` nor republishes
`mesh.settings`. The choice is lost on restart and can be reverted by the next
settings-file reload.

Inactive themes are parsed once during startup. `ThemeEngine::set_active`
clones that cached value (`crates/core/foundation/theme/src/lib.rs:417`) and the
shell then records the source file's *current* modification time. If the file
changed after startup but before selection, the cached old CSS can remain
active indefinitely. The file watcher is also configured from startup paths;
after switching into another directory its polling is parked when the watcher
is active, so later edits can be missed.

**Improve it:** persist selection through the settings/profile transaction,
then prepare from a fresh descriptor fingerprint before swapping. Rebind the
watch set on every catalog/selection change and keep the active and catalog
snapshots on the same revision.

### 6. High — the theme CSS parser accepts rules the resolver cannot execute

The theme crate implements a second CSS parser with comment removal, raw brace
scanning, and `split(';')`
(`crates/core/foundation/theme/src/lib.rs:631`, `:647`, and `:800`). It stores
every non-`node` selector as an exact component-default key (`:708`). The style
resolver only requests `base` and the bare element tag
(`crates/core/ui/elements/src/style/resolve/declaration.rs:106`). A promised
rule such as `button:hover` is therefore accepted but never matches. Selector
lists and unsupported selectors fail silently in the same way.

Unterminated comments and unmatched/trailing text can be truncated or ignored,
so malformed input may replace the last-known-good theme without a diagnostic.
The parser also cannot provide useful source locations or consistently validate
properties and values.

**Improve it:** extract a renderer-neutral restricted CSS syntax/lowering
package shared by component and theme CSS. Compile selectors to the same AST
and matcher, preserve declaration order, attach source spans, enforce resource
limits, and reject unsupported syntax instead of storing dead map keys.

### 7. High — theme keyframes and composable token semantics stop at special cases

Theme CSS parses `@keyframes`, but ordinary component animation looks only in
the component stylesheet (`crates/core/shell/src/shell/component/animation.rs:300`
and `:390`). Theme keyframes are consumed only by tooltip-specific code
(`shell/component/tooltip.rs:52`). A theme default which assigns a general
animation therefore reaches an “unresolved animation” diagnostic.

Token aliases/recipe tokens are also not recursively resolved through the
ordinary pure-`var()` lookup: a theme token string containing another
`var(--...)` can reach typed color/number/animation parsing still unresolved
(`crates/core/ui/elements/src/style/resolve/value.rs:117`). In local component
styles, custom-property scratch state is rebuilt per node rather than inherited
through the subtree, so parent custom properties do not provide the CSS-like
descendant cascade described by the specification.

**Improve it:** compile pack and component keyframes into one shared animation
registry, and resolve a typed token dependency graph once per snapshot with
cycle and missing-reference diagnostics. Carry inherited custom properties as
part of the style cascade rather than per-node scratch state.

### 8. Medium — observable theme state can disagree with rendered state

A valid same-ID CSS reload repaints components but only republishes
`mesh.theme` state when the ID changes
(`crates/core/shell/src/shell/runtime/theme.rs:124`). Palette/token observers
therefore retain old data. The shell then attributes its derived theme snapshot
to whichever service provider is selected, allowing later provider state to
overwrite facts about the core-rendered snapshot.

Dark/light status is guessed from the ID containing `dark`, with a Tokyo Night
special case (`runtime/theme.rs:8`). It fails for arbitrary names and makes
multi-mode themes impossible to report correctly. Service events omit mode,
snapshot revision, changed tokens, and source provenance.

**Improve it:** make the rendered `ThemeSnapshot` authoritative for theme facts.
Publish one coherent state/event per committed revision containing theme,
mode, color scheme/contrast, revision, and effective token delta. Providers may
request policy changes, but cannot overwrite the snapshot being rendered.

### 9. Medium — mutable/duplicate theme state weakens deterministic caching

`ThemeEngine::register_theme` accepts duplicate IDs and `set_active` selects the
first match (`crates/core/foundation/theme/src/lib.rs:413`). Equal-ID ordering
depends on filesystem enumeration, duplicate service rows remain possible, and
switching can restore an older cached copy.

`Theme` claims every style-bearing mutation advances its revision, but
`keyframes` is public. Module tokens are also flattened into the root map once;
after `modules_mut`, explicit module lookup can observe the stale flattened
value before the current scoped value (`theme/src/lib.rs:380` and `:261`).

**Improve it:** make snapshots immutable after construction, use a
deterministic identity map which rejects ambiguous IDs, keep scoped tokens only
in scoped storage, and derive a new revision from each successfully composed
snapshot rather than exposing mutable maps.

## Recommended target architecture

```text
InstalledModuleGraph
  └─ authorized ThemePackDescriptor catalog
       { scoped id, owner, modes, default, contained sources, provenance }
                         │
                         ▼
ThemeCoordinator::prepare(graph, selection, settings)
  ├─ read selected mode through module-owned source handle
  ├─ parse with shared restricted CSS frontend
  ├─ compose base + pack/mode + owned module layers + user overrides
  ├─ type/check tokens, selectors, properties, keyframes and cycles
  └─ produce Arc<ThemeSnapshot>
       { revision, mode, tokens, rules, keyframes, sources, diagnostics }
                         │
              atomic last-known-good commit
                         │
       ┌─────────────────┼──────────────────┐
       ▼                 ▼                  ▼
 style resolver     animation registry   mesh.theme/settings state
 dependency-aware   shared keyframes     revisioned events + provenance
 invalidation
```

A useful feature beyond the current flow is an explicit mode policy
(`manual`, `follow-system`, or `schedule`). It should sit above the same
transactional coordinator, so portal/system changes use exactly the same
validation, persistence, and event path as a manual switch.

## Recommended implementation order

1. Add regressions for fatal reload, non-durable/stale selection, traversal,
   pseudo-state rules, general theme keyframes, same-ID publication, and
   duplicate IDs.
2. Introduce the graph-authorized descriptor/catalog and contained source
   opening; migrate shipped themes and delete the parallel loader formats.
3. Extract the shared restricted CSS parser/lowerer and typed token/keyframe
   representation.
4. Build the pure composer and immutable snapshot with mode, module/user
   layers, provenance, and complete validation.
5. Make startup, profile switch, settings/service writes, graph changes, and
   file reload prepare then atomically commit through one coordinator.
6. Complete the settings and `mesh.theme` contracts, durable selection,
   revisioned events, explicit color-scheme metadata, and watcher rebinding.
7. Feed compiled theme selectors/keyframes and inherited variables into the
   shared style/animation paths; then use token dependencies for narrower
   invalidation.
8. Add CLI/LSP/doctor support for catalog, modes, effective tokens, provenance,
   and coherence diagnostics.

## Required regression coverage

- Canonical installed theme modules load through the graph; the private
  `kind: theme` + `mesh.theme` shape is a migration diagnostic and untracked
  theme directories are not activation inputs.
- Missing/disabled/ambiguous theme or mode, duplicate IDs, invalid default
  mode, missing file, symlink escape, oversized input, invalid UTF-8, and bad
  CSS fail candidate preparation without changing live state.
- Absolute/parent-path requests never open outside an owning module root.
- A profile switch with an invalid theme leaves the old profile, surfaces,
  providers, settings, and theme revision intact.
- Pack defaults, owned module contributions, and user overrides compose in the
  documented order; a module layer never affects another module's subtree, and
  provenance identifies the winning layer.
- Theme/mode/token changes persist in the correct scope, survive restart, and
  reject stale expected revisions without mutating the live snapshot.
- Switching to an inactive edited theme loads current content; watcher paths
  follow selection; same-ID edits update rendering, catalog palette, and
  service revision; malformed edits retain the old snapshot and recover later.
- Theme pseudo-states match through the normal selector engine; unsupported
  selectors/properties are source-located errors; quoted braces/semicolons and
  unterminated syntax are handled correctly.
- General nodes resolve theme keyframes; token aliases/recipes resolve
  transitively; cycles fail; inherited custom properties reach descendants.
- Every surface and observer sees one identical committed revision, even when
  a subscriber fails; theme facts cannot be overwritten by provider updates.

## Verification

Four independent review passes reconstructed the flow, challenged its order and
feature design, inspected concrete code defects, and audited canonical module,
security, and transaction boundaries. One flow-mapping pass used Luna xhigh as
requested; no reviewer edited production code.

Executed locally with `nix develop`:

```text
mesh-core-theme: 12 passed
mesh-core-shell focused shell::tests::theme: 12 passed
mesh-core-elements focused theme-default resolution: 9 passed, 6 ignored benchmarks
```

These suites validate the current isolated behavior but do not cover the
cross-boundary failures listed above. No production code was changed by this
audit.
