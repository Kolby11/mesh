# MESH Specification

This directory is the **single unified specification** for the MESH module
platform. It supersedes the older scattered design docs (`module-system.md`,
`extensibility.md`, `installation.md`, `icon-system.md`, `font-system.md`,
`theming/`, `settings/`, `health.md`, `component-configuration.md`,
`module-vocabulary.md`, and the root `spec/pluggable-backend.md`), which have
been deleted. If an older document or commit message contradicts this spec,
this spec wins.

Start with [00 — Platform Philosophy](00-philosophy.md), the canonical source
for vocabulary and ownership rules confirmed on 2026-09-05. Detailed chapters
define schemas and implementation status; they do not maintain a separate
philosophy. An undecided option is not a committed target.

Each part is marked with an implementation status per section:

- **Shipped** — implemented and tested in the current tree.
- **Target** — decided design, not yet (fully) implemented.

The confirmed direction records (2026-07-01 and 2026-07-16) encode one sparse
settings model, user icon-pack modules and pack chains, props everywhere,
closed core plus open contribution namespaces, path and Git installation,
load-time theme cascade, minimal font packs, automation IPC, and thin MCP.

The 2026-07-16 platform direction further establishes MESH as a shell-building
platform: directly editable modules provide components and services; one module
exports one primary public unit with explicitly declared public contributions;
named profiles compose root components and
service choices; configuration is profile-scoped while durable service data is
shared; and settings, developer tools, and package experiences are replaceable
modules using built-in core services. The September clarification separates
replaceable UI from core-owned settings/storage and management mechanisms.

## Parts

| Part | Covers |
| ---- | ------ |
| [00 — Platform Philosophy](00-philosophy.md) | Core/module boundary, vocabulary, element standards, configuration ownership, isolation, and language direction |
| [01 — Module System](01-module-system.md) | Vocabulary, `module.json`, kinds, contracts, providers, profiles, capabilities, lifecycle, trust |
| [02 — Installation & Health](02-installation.md) | Installer v1 (path + git), directories, doctor, health states, diagnostics |
| [03 — Components & Props](03-components.md) | `.mesh` component model, the `<props>` block, projections, precedence |
| [04 — Styling & Theming](04-styling.md) | Theme packs, tokens, load-time cascade, module theme contributions, modes |
| [05 — Icons](05-icons.md) | Semantic names, icon-pack modules, vocabulary index, resolution chain, variable axes |
| [06 — Fonts](06-fonts.md) | Font-pack modules, logical roles, `--font-*` tokens, resolution |
| [07 — Localization](07-i18n.md) | Module catalogs, language packs, lookup chain, plurals, RTL |
| [08 — Settings](08-settings.md) | The single sparse settings store, namespaces, precedence, generated UI |
| [09 — Accessibility](09-accessibility.md) | Semantic tree, roles, names, states, AccessKit, diagnostics |
| [10 — Keyboard](10-keyboard.md) | Focus traversal, activation keys, keybind contributions, `keyboard_mode` |
| [11 — Automation IPC](11-automation-ipc.md) | Capability-gated IPC: semantic tree, element actions, surfaces, settings |
| [12 — MCP for LLMs](12-mcp.md) | The thin `mesh-mcp` binary over the automation IPC |

## How the parts compose

```
                 shell profile (01)
                              │
                              ▼
                       module.json  (01)
        identity · kind · uses · provides · implements
                              │
        ┌──────────┬──────────┼───────────┬─────────────┐
        ▼          ▼          ▼           ▼             ▼
   interfaces   resource    .mesh      keybinds     capabilities
   + providers   packs    components     (10)          (01)
      (01)     (05,06,07)  + props (03)
        │          │          │
        │          │          ▼
        │          │     <props> ──► prop() CSS · props.* Lua · settings rows (08)
        │          │          │
        ▼          ▼          ▼
   settings store (08) ◄── user decisions: theme/mode, pack chains, locale,
        │                   keybind overrides, per-module + per-instance props
        ▼
   theme cascade (04) ──► style resolution ──► semantic tree (09)
                                                    │
                                     keyboard focus (10) ─ AccessKit (09)
                                                    │
                                        automation IPC (11) ──► mesh-mcp (12)
```

The shared principles live in [00](00-philosophy.md): core owns platform
invariants, modules own experiences, elements enforce shared standards, and
profiles compose. Settings remain sparse while scripts control effective props
with access to user-layer values. Resource resolution uses semantic names and
declared fallback rules. See the detailed chapters for each resolution ladder.

## Related reference docs (not part of this spec)

- [`../architecture/overview.md`](../architecture/overview.md) — codebase and
  runtime orientation.
- [`../frontend/mesh-syntax.md`](../frontend/mesh-syntax.md) — `.mesh` syntax reference.
- [`../frontend/elements.md`](../frontend/elements.md) — native element taxonomy.
- [`../modules/README.md`](../modules/README.md) — shipped module index.
