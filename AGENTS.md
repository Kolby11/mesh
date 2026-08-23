# MESH

Read first: [`docs/architecture/overview.md`](docs/architecture/overview.md) and
[`docs/spec/README.md`](docs/spec/README.md).

---

## Where work is tracked

**Check these at the start of any session that changes code.**

| Question | File |
| --- | --- |
| What should I work on? | [`docs/BACKLOG.md`](docs/BACKLOG.md) — the single list of open work |
| What is in flight right now? | [`.planning/STATUS.md`](.planning/STATUS.md) |
| Has this been tried before? | [`.planning/log/`](.planning/log/README.md) — dated history and measurements |

There is **one** backlog. Never create a competing TODO list, task file, or
progress tracker anywhere else in the repo, and never add a "TODO" section to a
doc — put the item in `docs/BACKLOG.md` or it does not exist.

### When you finish a piece of work

1. **Delete the item from `docs/BACKLOG.md`.** Do not mark it `[x]` and leave
   it — completed items leave the file entirely.
2. **Write its record in the log.** Performance work goes in
   [`.planning/log/performance-log.md`](.planning/log/performance-log.md);
   everything else goes in the current month's `.planning/log/YYYY-MM.md`.
   Create that file if the month has none.
3. **Update [`.planning/STATUS.md`](.planning/STATUS.md)** if what is in flight
   changed. It is short and disposable — overwrite it.

### When you start something new that isn't in the backlog

Add it to `docs/BACKLOG.md` first, as one to three lines: what to do, and why
it is not done. If it needs a design before it can be executed, write that in
`.planning/todos/pending/` and link to it from the backlog line.

### Rules the files enforce

- **The backlog says what is open, not what happened.** No dated progress
  paragraphs, no benchmark numbers, no completed items. It grew to 1,250 lines
  once by ignoring this; keep items to one to three lines.
- **The log is append-only.** Never edit a past entry. Correct a wrong one with
  a new dated entry that links back — past entries hold the baselines later work
  is compared against.
- **Every performance claim carries its measurement**: build profile, workload
  shape and size, before/after as ranges across repeated runs, and the gate name
  if one exists. A speedup without its workload is not reproducible.
- **Record what failed.** An approach that was measured and reverted is worth
  more than one never tried. Check the rejected-experiments table in
  `.planning/log/performance-log.md` *before* attempting an obvious
  optimization — several are already known dead ends.
- **`.planning/archive/` is frozen.** Never update it, and never treat it as
  current — where it contradicts `docs/spec/`, it is out of date.

Full rules: [`.planning/README.md`](.planning/README.md).

---

MESH is a Wayland-only shell framework built in Rust.

It is not a compositor or window manager. It runs on top of existing Wayland compositors and provides shell UI such as panels, launchers, notifications, quick settings, overlays, widgets, and settings surfaces.

## Core idea

MESH is a platform for building desktop shell experiences with:

- Rust core runtime
- Luau scripting for extensions
- single-file UI components
- XHTML-like markup
- CSS-like styling
- editable module ecosystem
- system-wide theme inheritance
- accessibility-first component model
- localization support
- typed external configuration

## Critical terminology

Use these terms precisely in code, documentation, and architecture discussions:

- **Module**: the installable unit for MESH. Modules use a canonical
  `module.json` with all MESH behavior under the `mesh` key. `mesh.kind`
  describes the role (`frontend`, `backend`, `interface`, `theme`,
  `icon-pack`, `font-pack`, `language-pack`, or `library`). Old manifest
  inputs (`package.json`, `mesh.toml`, legacy top-level `id/type` fields) are
  rejected with migration diagnostics — not accepted as compatibility inputs.
  `mesh.kind` also includes `component`: an embeddable `.mesh` component
  consumed by other modules via `require("@scope/name")`, with no
  `mesh.surface` block of its own. `mesh.kind: "composition"` is a profile
  that is also a module: it binds root component instances, surface
  placement, provider/resource selection, and slots into one named,
  installable, versioned shell family, but declares no `entry`,
  `mesh.surface`, `mesh.implements`, or capabilities of its own.
- **Element**: a base UI primitive exposed by MESH core, such as `box`,
  `row`, `button`, `icon`, `input`, `slider`, or `text`. Elements are the
  built-in building blocks with predefined runtime behavior, styling hooks,
  accessibility handling, layout participation, event handling, and Lua-facing
  functionality.
- **Component**: a user-authored reusable `.mesh` unit made from base elements
  and, optionally, other components. Components encapsulate markup, Luau state
  and handlers, styles, schema, translations, and metadata. A component is not
  a core primitive.
- **Frontend/component module**: an installable UI unit with one primary public
  `.mesh` component. It may contain private internal components. A shell profile
  can mount the public component as a surface or another component can embed it.
- **Interface**: a named, versioned contract distributed as an `interface`
  module. Backends implement interfaces; frontends consume interfaces; the
  core validates and routes calls without knowing service-specific behavior.
- **Luau library module**: a module that contributes importable Luau helpers
  for backend and frontend scripts. Libraries reduce repeated parsing,
  polling, formatting, and result-shaping code, but they do not grant
  capabilities by themselves.

When modeling Luau or LSP APIs, prefer this hierarchy: core **elements** expose
the base typed API; user **components** compose elements; a module exports one
primary public component or service.

For the target module direction, see `docs/spec/01-module-system.md`.

## Main goals

- extensible shell platform, not hardcoded widgets
- Wayland-native shell surfaces
- downloadable widgets, services, themes, and language packs
- Material 3 inspired token-based theming
- reusable components with public settings and style hooks
- semantic metadata for accessibility and AI interaction

## Non-goals

- no custom compositor
- no custom window manager
- no full control over compositor policy
- no guaranteed identical behavior across all Wayland compositors

## Main architecture

### 1. Rust core

Responsible for:

- lifecycle
- module loading and contract validation
- settings
- theming
- localization
- permissions / capabilities
- IPC / event bus
- diagnostics
- component compilation
- runtime coordination

### 2. Wayland frontend

Responsible for frontend modules that implement shell surfaces and widgets such as:

- panel
- launcher
- notification center
- control center
- overlays
- settings windows

### 3. Extension runtime

Embeds Luau and provides sandboxed host APIs.

Extensions can implement:

- widgets
- shell surfaces
- services
- themes
- language packs

Backend/service modules must be implemented in the module's scripting language
through the extension runtime host API, not in Rust shell code. If a module
needs a new system capability, add a generic host API to the runtime and keep
the service-specific logic inside the module script.

Luau execution should go through a real runtime library, not hand-written
string parsing. Use `mlua` in Luau mode for script execution and treat any
custom parsing/interpreting as temporary migration code to remove, not a model
to expand.

Extension authoring should prefer normal Lua/Luau syntax and semantics by
default. Only introduce special parsing, custom DSL behavior, magic globals, or
non-standard syntax when there is a clear product need that cannot be met
cleanly through regular host APIs or standard language constructs.

Backend `main.luau` files should expose an explicit `start(self)` entrypoint
function. Backend setup such as poll interval registration should happen inside
`start()` rather than relying on top-level side effects.

### 4. UI component format

Single-file user components inspired by Svelte. Components are authored from
MESH core elements and other components.

Conceptual blocks:

- `<props>` (target config model; see below)
- `<template>`
- `<script lang="luau">`
- `<style>`

The `<props>` block is the planned single declaration for typed, defaulted,
localized component configuration. One entry auto-projects to a `prop(name)` CSS
reference, a reactive `props.name` Lua field, and a generated settings UI row —
replacing scattered `mesh.surface` sizing and `mesh.settings`. This is a **design
spec, not yet implemented**: see `docs/spec/03-components.md`.

## Core module kinds

- frontend
- backend
- interface
- component
- library
- composition
- theme
- icon-pack
- font-pack
- language-pack

Each module declares identity (`name`, `version`), `mesh.kind`, dependencies
and capabilities (`mesh.uses`), contributions (`mesh.provides`), provider
records (`mesh.implements`), entrypoints, and i18n metadata. See
`docs/spec/01-module-system.md`.

## Key concepts

### Shell surface
Top-level shell UI like panel, launcher, or notification drawer.

### Widget
Embeddable UI component inside a shell surface.

### Service
Structured provider of state/actions such as battery, media, network, notifications, AI, theme, or locale.

## Extension model

Capability-based. Packages must explicitly request access.

Examples:

- `shell.surface`
- `shell.widget`
- `service.network.read`
- `service.media.read`
- `service.notifications.post`
- `theme.read`
- `locale.read`
- `exec.launch-app`

## Theming

Token-based, shell-wide inheritance.

Suggested token groups:

- colors
- typography
- spacing
- radius
- elevation
- borders
- motion
- shadows

Components inherit tokens by default and may expose controlled style variables.

## Accessibility

Required by design.

Components should expose:

- role
- label
- description
- state
- focus metadata
- keyboard behavior
- localizable text

The semantic tree should also support automation and AI interaction.

## Localization

System-wide and module-aware:

- language packs
- module translations
- locale switching
- fallback chains
- pluralization / formatting

## Configuration

Every component should expose typed public settings so the shell can generate settings UIs and validate user configuration.

## Performance strategy

- compile components
- avoid overly dynamic runtime interpretation
- keep rendering host-driven
- isolate or throttle bad extensions
- minimize redraw and idle overhead

## Security strategy

- sandbox Luau runtime
- signed modules
- capability-based permissions
- limited host APIs
- trust levels and install-time review

## First version

Target:

- top panel
- launcher
- notification center
- quick settings
- theme engine
- module installer/package-service experience
- widget SDK
- service SDK
- localization support
- accessibility-aware component model

## One-line definition

MESH is a Rust-based, Wayland-native shell-building platform with editable,
theme-inheriting, accessibility-first modules and Luau-powered services.

## Tracking work

See [Where work is tracked](#where-work-is-tracked) at the top of this file.
