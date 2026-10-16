# Module architecture direction

**Status:** proposal, 2026-09-30. Agreed in discussion; not yet reflected in
`docs/spec/`. Each section becomes a spec change plus implementation when it is
scheduled from the backlog. Evidence for the related defects is in the
[2026-09-30 block review](../../codebase/audits/2026-09-30-block-review/REPORT.md).

The goal is a module ecosystem other people can extend: modules from different
authors interoperate, users customize without forking, and backends remain
fully user-creatable. Nothing here moves domain services into core.

## A2. Optional features inside a contract

**Problem.** Semver plus "prefer `mesh.audio.v2`" makes every new ability either
a breaking change or an ecosystem split, while real providers differ in what
they can do.

**Direction.** Contracts group optional members into named features
(`features: { "per-app-volume": [...] }`). Providers declare supported features;
consumers call `audio:supports("per-app-volume")`. Adding a feature is a minor
version; a provider without it degrades cleanly. Validation rejects use of an
unsupported feature's members.

## A3. Style parts and a user stylesheet layer

**Problem.** Customization stops at theme tokens and declared props. Anything
else requires editing module source, which updates then overwrite.

**Direction.** Authors mark public styling targets (`<icon part="glyph">`). A
profile-scoped user stylesheet may target them
(`@mesh/navigation-bar::part(glyph) { … }`). Parts are the module's versioned
styling API, like props; unmarked elements stay private. Customization tiers
become props (settings UI), parts (CSS), fork (A4) — only the last breaks
updates. LSP completes parts; doctor warns on removed parts.

## A4. Tracked forks instead of in-place edits

**Problem.** "Directly editable modules" conflicts with receiving updates;
`config eject` only materializes settings.

**Direction.** `mesh-shell module fork <id>` copies a module into a user scope,
records the upstream id and version, and rewires the active profile to the
copy. `module diff` / `module rebase` show upstream changes against the fork.
The original stays installed as a fallback.

## A6. Widgets independent of bars

**Problem.** `mesh.navigation.item` exists, but shipped bar widgets (clock,
volume, workspaces) are private components of `navigation-bar`, and volume
presentation logic is copied into four frontends (RES-05).

**Direction.** Ship bar widgets as separate modules contributing to
`mesh.navigation.item`, with per-instance props. Any bar can host them; the
settings UI can offer placement between bars because placement is profile
data. Shared presentation helpers ship as a library or component module, not
a core element (00 §4).

## A8. Make high-risk capabilities rare

**Problem.** Most shipped backends need `exec.argv` grants because the platform
lacks D-Bus, file and protocol primitives. Every install shows alarming
permissions, and argv globs are hard to keep narrow (MOD-04). CLI scraping is
also locale-fragile (RES-07).

**Direction.** Prioritize scoped host primitives: D-Bus calls and signals scoped
per bus name/interface (extends the existing backlog item), bounded file
read/watch, and Wayland protocol data (ext-workspace, foreign-toplevel).
Present permissions in plain terms ("control audio", "read battery status",
"run any command") so the rare high-risk grant stands out.

## What stays

Contributions name contracts, never hosts; hosting is explicit; profiles are
sparse deltas over compositions; one default public unit per module plus
explicit contributions; core owns settings, storage and transactions while
their UIs are modules.

## Order

A1, A7 and A5 have landed (see the October log). Next A3 and A6 (largest
visible customization wins), A2, A8, and A4.
