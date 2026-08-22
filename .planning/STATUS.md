# Status

**Updated:** 2026-08-22

## Now

Localized runtime misses now use the same owner/key/fallback/source/snapshot
resolution record as manifest metadata. Luau and template consumers render
`!!key`, enqueue structured observations, and shell diagnostics deduplicate
them by module/key; manifest fields retain their field path in the resolution.

The installed graph now exposes one ordered, profile-aware locale read model for
runtime and tools. CLI list/active/set/which/missing/extract/doctor commands,
LSP locale completion, and static key extraction consume that model. Component
files reject inline `<i18n>` blocks with a source-line migration diagnostic;
catalogs belong in `mesh.provides.i18n`.

Locale policy is now explicit (`manual` or `follow_system`), resolved only for
runtime consumers, and persisted through shared or active-profile revision
transactions. `locale set` selects manual mode; `locale set-system` uses the
same catalog preparation and commit path for follow-system mode. The locale
service and LSP settings schema expose the policy.

Resource activation now derives ordered icon/font pack chains and contained
icon/font contribution paths from the prepared graph/profile. Icon-pack
bindings and graph-authorized frontend bindings are prepared together and
atomically replace the live icon registry; a revisioned resource snapshot is
retained alongside the live graph, and failed candidates leave it untouched.

Resource chains preserve declared order and reject duplicate or inactive
entries. Atomic icon binding replacement requires canonical pack IDs and
rejects duplicate pack/module ownership; font aliases resolve only within the
pack that owns each mapping, and duplicate aliases are rejected during
preparation. Module assets are validated through contained, no-follow,
bounded handles before entering a candidate snapshot. Complete selected icon
packs now read and validate bundled fonts and glyph maps on the resource
preparation worker, retain parsed glyph maps in immutable bindings, reject
malformed mappings or missing glyphs as whole-candidate failures, and publish
only through the atomic registry replacement. Icon fallback resolution uses
the canonical semantic table and dash generalization, while typed multicolor
mappings preserve source color and successful resolutions retain
owner/pack/candidate/fallback provenance.

Resource caches now share a monotonic revision and metadata fingerprint.
Atomic icon binding publication advances the revision; registry and XDG
negative lookups, glyph maps, variable-font axes, icon font bytes, glyph
rasters, file image/raster caches, text layout/font-system state, and ellipsis
shaping keys all carry the revision/fingerprint needed to reject stale
results. Focused resource, icon, text, glyph, renderer, and config tests pass;
the full config suite retains one existing repository-fixture failure for its
top-level `revision` metadata, and the broader shell icon slice retains three
existing navigation integration failures. The shell still emits its existing
dead-code/private-interface warnings.

Font-pack runtime resolution is complete. Manifests carry validated role
mappings, soft host-font requirements, bundled face metadata, and script
coverage. Resource preparation translates profile and module chains to
pack-qualified IDs, validates contained font bytes off the shell/render path,
and publishes an ordered `FontRegistry` with prepared font databases. Logical
roles generate `--font-*` typography tokens, `pack/role` references use an
internal theme binding, and per-module overrides retain exact-family and
system-fallback resolution with coverage and missing-requirement diagnostics.

The host resource catalog now owns ordered XDG data, icon, and font roots,
theme inheritance order, and the shared host font database. Icon discovery and
lookup, icon-pack host-font resolution, and resource preparation consume the
same immutable refreshable snapshot; a changed host catalog is published with
the resource revision while unchanged refreshes retain the existing snapshot.

The next open item is to prepare resource parsing and asset handles away from
shell/render threads with bounded cancellation.
