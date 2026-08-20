# Section 6 — Host resources and icon packs: improvement audit

**Audited:** 2026-08-19
**Scope:** `mesh-core-resources`, `mesh-core-icon`, canonical icon/font-pack
manifests, graph/profile/settings activation, XDG and font discovery, semantic
resolution, renderer caches, module asset safety, diagnostics, and CLI/LSP
tooling.

This is a point-in-time review record, not a second task tracker. Open work from
this audit lives in [`docs/BACKLOG.md`](../../../../docs/BACKLOG.md).

## Logical process map

The shipped path discovers and publishes resources before the active graph and
profile have authorized them. Host discovery, module registration, profile
selection, and rendering therefore never become one coherent revision:

```text
host environment                         every module found on disk
  XDG data dirs + font database             module.json / legacy assets.icons
           │                                           │
           ▼                                           ▼
 SystemResourceCatalog                           shell discovery
 process-wide OnceLock                       (before active graph filtering)
   ├─ icon theme metadata                          ├─ register icon pack
   └─ font family names                            └─ register frontend bindings
           │                                           │
           └──────────────┬────────────────────────────┘
                          ▼
                  process-global IconRegistry
                    ├─ user override
                    ├─ author override
                    ├─ pack-qualified target
                    ├─ frontend/default pack chain
                    └─ hicolor / XDG fallback
                          │
                          ▼
                 file or font-glyph target
                          │
                          ▼
             renderer SVG/raster/font caches → Wayland

active InstalledModuleGraph + ShellProfile.resources + sparse settings
   ├─ select mounted frontends/providers
   ├─ apply theme and icons.default_pack only
   └─ do not build the effective icon/font resource registry above

profile switch / uninstall / module edit / host icon or font change
   ╳ no complete candidate, removal reconciliation, atomic commit,
     resource revision, or last-known-good rollback
```

Text fonts follow a separate path: typography tokens carry an exact family
name into a thread-local `cosmic-text` font system. The declared font-pack role,
chain, and pack-qualified model is not connected to that path.

## Confirmed findings

### 1. Critical — the runtime resource registry bypasses graph/profile ownership

Module discovery registers icon packs and frontend bindings for every manifest
found on disk before active graph reconciliation
(`crates/core/shell/src/shell/discovery.rs:479` and `:548`). Profile preparation
does not build a resource candidate, and commit only updates the shell default
pack setting (`shell/profile.rs:442` and `:783`). Profile `resources.icons` and
`resources.fonts` never compose the resolver.

Removal APIs exist in `mesh-core-icon`, but the shell does not call them during
uninstall, unload, or profile switching. Disabled, superseded, or uninstalled
providers can consequently remain globally resolvable, and a failed change can
leave mixed graph and resource state. Settings reload updates only the shell
default pack; it does not rebuild per-module `use_packs`, overrides, or
`ignore_shell_default` bindings, so those settings remain stale until restart
(`shell/runtime/theme.rs:362`).

**Improve it:** build resources only from the prepared active graph and profile.
Validate a complete candidate and atomically replace the old registry alongside
graph/lifecycle commit, retaining the last-known-good snapshot on failure.

### 2. Critical — pack order, registry identity, and aliases are nondeterministic

Canonical `mesh.uses.resources.icons` starts as an ordered `Vec`, but conversion
to runtime dependencies inserts it into a `HashMap` and reconstructs the pack
list from unordered keys
(`crates/core/extension/module/src/package/module_manifest.rs:764`, `:869`,
`:907`, and `:952`). Conflicting packs can therefore violate the documented
“earlier entries win” rule before they reach the resolver.

Re-registering one module under a new pack ID updates `pack_id_by_module` but
does not delete its prior `icon_packs` entry
(`crates/core/ui/icon/src/registry.rs:124`). Duplicate IDs can replace an owner
instead of failing. Font targets such as `ms/settings` then search aliases
across every pack through `HashMap` iteration (`registry.rs:405`), so a mapping
owned by pack A can consume pack B's alias and collision results depend on map
order.

**Improve it:** preserve declared order in an order-bearing type, compile
canonical bidirectional ownership indexes, reject duplicate pack IDs and
ambiguous aliases before activation, scope aliases to their owning pack/module,
and remove all superseded identities in the same snapshot swap.

### 3. Critical — module resource paths can escape their root

Canonical icon-pack font and glyph-map fields are described as module-relative,
but registration directly joins them to the module directory without the
module manifest's relative-path validator (`crates/core/shell/src/shell/mod.rs:255`).
Legacy asset paths are joined similarly. `resolve_pack_path` explicitly accepts
absolute paths, `~/`, and parent traversal
(`crates/core/ui/icon/src/xdg.rs:197`). Git installs can also retain symlinks,
and later `is_file` checks do not prevent replacement before render-time reads.

SVG external references are detected only to disable raster caching; rendering
still supplies the asset's parent as `resources_dir`
(`crates/core/frontend/render/src/surface/icon.rs:242` and `:543`). “Not cached”
is not an isolation boundary.

**Improve it:** turn module assets into contained, no-follow, regular-file
handles during candidate preparation; reject traversal, absolute paths,
symlinks, and external SVG references for untrusted modules. Keep explicitly
authorized user overrides as a separate path type. The canonical backlog's
module-filesystem item owns the shared containment mechanism.

### 4. High — legacy semantic fallbacks are constructed but never resolved

`default_icon_config()` builds the older semantic fallback table, including
names such as `audio-volume-high → volume`, but `resolve_uncached` never
consults `active_profile().icons` (`crates/core/ui/icon/src/lib.rs:17` and
`registry.rs:271`). `config/icons.toml` remains present and tested even though
the current specification says this authority was deleted.

At the same time, the runtime retains both canonical manifest bindings and a
legacy `mesh-pack.json` discovery format. These parallel authorities make it
unclear which vocabulary and fallback contract is real.

**Improve it:** migrate the useful fallback vocabulary into the canonical
compiled chain, then delete the dead config and legacy discovery authority.
Unknown legacy inputs should receive an explicit migration diagnostic rather
than silently participating or being tested as current behavior.

### 5. High — the icon-pack contract loses multicolor and coverage semantics

Runtime mappings are `HashMap<String, String>`, so per-mapping multicolor
metadata cannot be represented (`crates/core/ui/icon/src/bindings.rs:11`). File,
glyph, and system results normally set `multicolor: false`, causing the renderer
to tint assets which the specification permits to preserve their colors.

The canonical runtime schema also lacks specified `kind`, `covers`, and
`vocabularies` fields. Dash-segment generalization remains a target gap, and
required/optional diagnostics check declarations rather than the effective
active chain and actual winning target.

**Improve it:** compile a typed mapping target with color policy, source kind,
coverage/vocabulary declarations, and provenance. Implement the documented
fallback sequence and validate required names against the exact active snapshot;
optional misses should remain observable without failing activation.

### 6. High — invalid packs can be partially published

Missing glyph maps and fonts produce warnings during registration, but aliases
are still inserted and the pack is still registered
(`crates/core/shell/src/shell/mod.rs:260`). Glyph-map JSON accepts the first
character of a multi-character value, while one malformed text line can abort
the entire map (`crates/core/ui/icon/src/xdg.rs:269` and `:277`). Pack metadata,
glyph maps, SVGs, and fonts are read without explicit byte, entry, or parse
complexity limits.

**Improve it:** parse and validate every resource off the render thread with
bounded input and per-entry diagnostics. Publish the pack only when its required
assets form a valid candidate; a rejected update must leave the prior valid
snapshot active.

### 7. High — host, lookup, glyph, and text-font caches have no shared freshness

The host resource catalog is a process-wide `OnceLock`
(`crates/core/foundation/resources/src/lib.rs:32`). XDG lookup caches both hits
and misses without a filesystem revision, glyph maps are cached by path, and
font bytes/glyphs are also keyed by path rather than file identity
(`crates/core/ui/icon/src/xdg.rs:58` and `:213`;
`crates/core/frontend/render/src/surface/glyph.rs:46`). The thread-local text
font system and family-availability cache are initialized once
(`surface/text.rs:41` and `:996`). Resource roots are not part of the live file
watch set.

Installing an icon after a cached miss, replacing a glyph map or font in place,
or changing available host fonts can therefore remain invisible until restart.
The raster file cache already includes modification metadata, but that freshness
model stops at that one layer.

**Improve it:** make the host catalog refreshable and attach one monotonically
increasing resource revision plus content/file fingerprints to registry,
negative lookup, glyph, font, text layout, and renderer caches. Rebind watchers
when the active snapshot changes.

### 8. High — font packs are declared as a goal but have no runtime resolver

The graph recognizes `font-pack` as a module kind, but there is no font-pack
section, role registry, ordered profile/module chain, pack-qualified role, or
bundled font activation. The only shipped setting is an exact
`fonts.ui_family`, copied into three typography tokens; a value such as
`default/body` is treated as an unavailable OS family and falls back.

**Improve it:** add a font side of the same resource snapshot: validated faces,
logical roles, pack and module chains, explicit style/weight/stretch coverage,
pack-qualified references, fallback provenance, and generated `--font-*`
tokens. Host families and module-bundled faces should resolve through one
revisioned font database.

### 9. Medium — XDG discovery and lookup are separate, inconsistent authorities

The host catalog and `IconSearch` construct their own search views. The catalog
collects XDG `Inherits=` values into a `BTreeSet`, alphabetically reordering a
declared precedence list (`crates/core/foundation/resources/src/lib.rs:123`).
Font pack registration separately executes synchronous `fc-match` calls and
uses a filename heuristic, which can disagree with the catalog's font database.
Empty or relative `XDG_DATA_HOME`, `XDG_DATA_DIRS`, and `HOME` entries are also
accepted as paths, allowing the current working directory to become an
accidental lookup authority
(`crates/core/foundation/resources/src/lib.rs:71`).

**Improve it:** prepare one ordered host snapshot containing XDG roots, theme
inheritance, icon locations, and font face metadata. Discovery, resolution,
diagnostics, and render preparation should consume that same snapshot; preserve
source-declared order and avoid per-pack subprocesses.

### 10. Medium — resource work can block startup and frame rendering

Fontconfig subprocesses run synchronously during registration. XDG searches and
glyph-map reads can happen during resolution; SVG and font files are read during
rendering. Slow or remote data directories and oversized assets can therefore
stall shell startup or a paint.

**Improve it:** resolve, read, parse, and validate into immutable asset handles
on worker threads. Render commands should consume prepared targets only. The
existing cross-cutting blocking-I/O backlog item owns the scheduling mechanism,
so this audit does not create a duplicate task for it.

### 11. Medium — diagnostics and tooling cannot explain effective resource state

`IconResolution::Found` lacks provider module, pack, chain position, asset
fingerprint, and fallback stage. Graph diagnostics can accept a mapping from any
enabled pack without proving that the effective profile chain can resolve it.
Missing diagnostics enumerate unordered registered maps, including providers
which should not be active.

The specified icon vocabulary, missing-name, resolve/which, pack validation,
and font role inspection commands are not implemented. LSP and doctor therefore
cannot share the runtime's effective answer or show why a candidate was
shadowed, rejected, or selected. The settings LSP advertises installed icon-pack
module IDs for `shell.icons.default_pack`, while the runtime setter accepts only
visible system XDG theme IDs (`crates/tools/lsp/src/settings/schema.rs:186` and
`crates/core/shell/src/shell/runtime/theme.rs:196`), so an offered completion can
be rejected at runtime.

**Improve it:** retain complete provenance and validation status in the
snapshot, and generate runtime diagnostics, CLI, LSP, and doctor output from
that exact model.

## Recommended target architecture

```text
InstalledModuleGraph + active profile + sparse settings + host change revision
        │
        ▼
ResourceCoordinator::prepare(graph, profile, settings, prior_snapshot)
  ├─ select only active graph-authorized resource owners
  ├─ preserve ordered icon/font chains and explicit empty overrides
  ├─ reject duplicate IDs/aliases and invalid dependency requirements
  ├─ open contained, no-follow, bounded module assets
  ├─ compile typed icon mappings, font roles, and fallback tables
  ├─ capture ordered XDG themes and host/module font faces
  └─ produce Arc<ResourceSnapshot>
       { revision, owners, icon chains, font chains, prepared assets,
         fingerprints, provenance, structured diagnostics }
        │
        ▼
atomic last-known-good commit with graph/module lifecycle
        │
        ├─ module-scoped IconResolver and FontResolver handles
        ├─ one renderer resource generation and targeted cache invalidation
        ├─ refreshed filesystem/host watchers
        └─ CLI/LSP/doctor inspect the same snapshot
```

A useful feature beyond the current flow is a resource coverage advisor. It can
compare a profile's semantic vocabulary and font-script needs with candidate
packs, preview unresolved names/roles and fallback provenance, and suggest a
better chain before activation. It must never silently reorder the user's
chain; applying a suggestion should still enter the normal prepare/commit path.

## Recommended implementation order

1. Add regressions for inactive/uninstalled providers, profile-chain switching,
   stale pack IDs, duplicate aliases, dead semantic fallbacks, multicolor
   preservation, path escape, and invalid-candidate rollback.
2. Freeze canonical icon/font contribution and profile shapes, including typed
   mappings, roles, coverage, color policy, versions, and explicit empty-chain
   semantics; reject legacy runtime authorities with migration diagnostics.
3. Introduce contained prepared assets, bounded parsers, deterministic owner
   indexes, and the immutable `ResourceSnapshot`.
4. Move discovery-time global registration into graph/profile
   `prepare`/`commit`, atomically reconcile removals, and retain last-known-good
   state on package, profile, settings, or reload failure.
5. Complete semantic icon resolution: owner-scoped aliases, canonical fallbacks,
   pack-qualified names, multicolor, dash generalization, and required/optional
   diagnostics with provenance.
6. Implement the symmetric font registry, logical roles, ordered chains,
   bundled faces, typography token integration, and pack-qualified resolution.
7. Unify host discovery and lookup; preserve XDG order, remove per-pack
   `fc-match`, add watchers, and make every cache revision/fingerprint aware.
8. Move asset I/O and parsing off the shell/render threads, then drive debug,
   CLI, LSP, doctor, validation, and the coverage advisor from the snapshot.

## Required regression coverage

- Switching profiles A/B replaces both icon and font chains; an explicit empty
  chain clears inheritance; disabled, removed, and uninstalled packs cannot
  resolve.
- Re-registering a module with a new pack ID removes the old qualified ID;
  declared first-pack-wins order is preserved, and duplicate IDs and aliases
  fail deterministically without changing live state.
- Legacy semantic names resolve through the canonical fallback table, dash
  generalization follows documented order, and pack-qualified lookups do not
  cross pack ownership.
- Multicolor assets preserve source color while symbolic assets tint; mapping
  provenance reports owner, pack, candidate, fallback stage, and asset.
- Missing font, glyph map, malformed middle entry, oversized input, invalid
  Unicode, or invalid SVG rejects only the candidate and retains the prior
  valid snapshot.
- Absolute, parent, home-relative, symlinked, replaced-after-check, and
  external-reference module assets cannot escape the module root.
- A cached icon miss becomes a hit after installation; XDG theme order remains
  source-defined; glyph-map, font, and host-catalog changes invalidate all
  dependent resolution, glyph, layout, and render caches.
- Logical font roles and pack-qualified roles select the expected face across
  weight/style/script fallback, surface/profile switching, and host font
  install/removal.
- Required names/roles fail candidate validation while optional misses remain
  structured warnings; CLI, LSP, doctor, and runtime resolve the same snapshot.
- Editing a live module's `use_packs`, overrides, or `ignore_shell_default`
  replaces its binding immediately; every settings/LSP completion is accepted
  or rejected by the same identifier validator used at runtime.
- Slow resource roots and large-but-valid assets are prepared away from the
  shell/render threads, with bounded cancellation and no partial publication.

## Verification

Four Luna xhigh review passes reconstructed the instruction and resolution tree,
challenged its logical order and feature model, inspected direct code defects,
and audited lifecycle, filesystem, parser, cache, renderer, and tooling
boundaries. No reviewer edited production code.

Executed locally with `nix develop`:

```text
mesh-core-resources: 1 passed
mesh-core-icon: 18 passed, 3 ignored
mesh-core-module icon slice: 14 passed
mesh-core-shell icon slice: 4 passed, 1 failed on the already-recorded shipped
navigation layout baseline before establishing an icon-resolution regression
```

The passing suites validate isolated current behavior but do not cover the
cross-boundary failures above. No production code was changed by this audit.
