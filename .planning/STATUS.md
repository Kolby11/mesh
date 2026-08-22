# Status

**Updated:** 2026-08-23

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

Resource preparation now carries a cooperative cancellation token through
bounded module/host reads, glyph-map parsing, bundled font validation, font
registry construction, and shell icon-pack candidates. Cancellation cannot
publish partial state because resource registries remain candidate-built and
atomically committed.

Resource preparation now also has a shared generation coordinator. New
candidates cancel older leases, carry a monotonic generation to the shell
commit boundary, and stale generations are rejected before publication.

The shell worker is now exposed as a pollable `ResourcePreparationJob` with
cooperative cancellation, current-generation checks, and safe retirement;
the existing synchronous callers use its blocking wait. Profile switching now
stores that job in a pending candidate, polls it between shell turns with a
bounded wake-up, and only advances to frontend/backend candidate preparation
after the resource lease is still current. A newer resource-only profile
request cancels and rejects the superseded candidate; prepared leases retire on
commit, rejection, cancellation, or worker failure.

Cacheable file-backed bitmap and SVG icon misses now enqueue bounded decode and
raster jobs on a dedicated render worker. The paint path uses the built-in
missing-icon placeholder until the result is published, and the shell polls
completion to invalidate component paint without blocking the render thread.
The worker queue and cache handoff are revision/freshness keyed.

Font-pack glyph misses now enqueue bounded font-byte loading and swash
rasterization jobs on a dedicated render worker. Completed alpha masks enter
the revision/fingerprint-keyed glyph cache, while Skia A8 image creation and
upload remain on the render thread; the shell polls both queues and repaints
when either resource is ready. Scheduler polling does not initialize idle
workers.

External-resource SVGs now use the same bounded icon worker for SVG reads,
external-reference detection, linked-resource loading, and rasterization.
Their completed variants are delivered once through a render-thread handoff
but never entered into the persistent raster cache, preserving the existing
no-cache behavior for mutable linked assets. Frontend/backend candidate
preparation after the resource stage remains outside this increment.

The next open item is to unify these queues with a generation-aware resource
broker and explicit cancellation, then continue frontend/backend candidate
preparation after the resource stage.
