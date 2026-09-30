# Module architecture direction

**Status:** proposal, 2026-09-30. Agreed in discussion; not yet reflected in
`docs/spec/`. Each section becomes a spec change plus implementation when it is
scheduled from the backlog. Evidence for the related defects is in the
[2026-09-30 block review](../../codebase/audits/2026-09-30-block-review/REPORT.md).

The goal is a module ecosystem other people can extend: modules from different
authors interoperate, users customize without forking, and backends remain
fully user-creatable. Nothing here moves domain services into core.

## A1. Interfaces are ecosystem standards, owned by one module

**Problem.** `mesh.wm`, `mesh.power` and `mesh.brightness` are declared inline
inside the backends that implement them (`hyprland-wm`, `upower-power`,
`backlight-brightness`). The first implementation defines the standard, the
definition disappears with the implementation, and a second provider has to
copy it. Today a copy with a higher `provider_priority` replaces the contract
(`installed_graph/graph.rs:736`, MOD-01).

**Direction.** Separate the three roles:

| Role | Kind | Who writes it |
| --- | --- | --- |
| Contract | `interface` module: state, methods, events, types | whoever defines a domain |
| Provider | `backend` module implementing the contract | anyone |
| Consumer | `frontend`/`component` using the contract | anyone |

- Shared domains are declared only in interface modules. Providers declare
  `implements` plus a dependency on the interface module; they never redefine it.
- Each interface name has exactly one owning module. A second declaration is
  an install/graph error, not a priority contest.
- `mesh.*` names are the curated standard set, published as `@mesh/*-interface`
  and admitted by provenance, not by name prefix (MOD-02).
- Anyone may publish interfaces in their own namespace (`@alice/weather-interface`
  declaring `alice.weather`); others may implement and consume them without
  permission. A widely used community interface can later be adopted into `mesh.*`.
- Inline interfaces remain allowed only in the declaring module's own namespace,
  for private backend/frontend pairs.
- Core keeps only its authorities (`mesh.settings`, `mesh.theme`,
  `mesh.locale`, profile and package management).

**First steps.** Extract `mesh.wm`, `mesh.power`, `mesh.brightness` into
interface modules; enforce single ownership in graph build. Before freezing
`mesh.wm`, review it against how niri and Sway expose workspaces so the standard
is not Hyprland-shaped. Grow the standard catalog deliberately: notifications,
media (MPRIS), tray, network, bluetooth, clipboard.

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

## A5. Fewer module kinds

**Problem.** Ten kinds is a steep start; `frontend` versus `component` is a
distinction users do not care about (a frontend is a component with default
placement, and profiles can already mount either).

**Direction.** One UI kind with optional `mesh.surface` default placement.
Consider one resource-pack kind that may carry any mix of theme, icons, fonts
and translations, so a "Nord pack" is one install. Keep backend, interface,
composition and library. Per the no-backward-compat rule, old kinds are
rejected with a migration diagnostic, not aliased.

## A6. Widgets independent of bars

**Problem.** `mesh.navigation.item` exists, but shipped bar widgets (clock,
volume, workspaces) are private components of `navigation-bar`, and volume
presentation logic is copied into four frontends (RES-05).

**Direction.** Ship bar widgets as separate modules contributing to
`mesh.navigation.item`, with per-instance props. Any bar can host them; the
settings UI can offer placement between bars because placement is profile
data. Shared presentation helpers ship as a library or component module, not
a core element (00 §4).

## A7. The design-token vocabulary is a versioned contract

**Problem.** A module looks right under another author's theme only if both
agree on token names; today that agreement is whatever `tokyo-night/theme.css`
defines.

**Direction.** Publish the semantic token set (M3-style color roles, shape,
motion, typography) as a versioned contract. Themes declare the version they
implement; LSP and doctor warn when a module reads a token outside the
contract. Module-private tokens stay namespaced (`@me/mod.color.x`).

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

A1, A7 and A5 first: cheap now, expensive after third-party modules exist.
Then A3 and A6 (largest visible customization wins), A2, A8, and A4.
