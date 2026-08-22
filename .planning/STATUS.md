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
bounded handles before entering a candidate snapshot. Icon fallback resolution
uses the canonical semantic table and dash generalization, while typed
multicolor mappings preserve source color and successful resolutions retain
owner/pack/candidate/fallback provenance.

The next open item is to validate complete packs off the render thread and
publish them atomically without partial registry state. Focused resource,
icon, diagnostics, locale, and config suites pass; the full workspace now
passes `cargo check` and all test targets compile with `cargo test --workspace
--no-run`. The shell still emits its existing dead-code/private-interface
warnings.
