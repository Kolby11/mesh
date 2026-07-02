# MESH Specification

This directory is the **single unified specification** for the MESH module
platform. It supersedes the older scattered design docs (`module-system.md`,
`extensibility.md`, `installation.md`, `icon-system.md`, `font-system.md`,
`theming/`, `settings/`, `health.md`, `component-configuration.md`,
`module-vocabulary.md`, and the root `spec/pluggable-backend.md`), which have
been deleted. If an older document or commit message contradicts this spec,
this spec wins.

Each part is marked with an implementation status per section:

- **Shipped** — implemented and tested in the current tree.
- **Target** — decided design, not yet (fully) implemented.

The confirmed direction record (2026-07-01) that these specs encode:
one sparse settings store, user icon-pack module + pack chains, props
everywhere, closed core + open provides manifest, path+git installer,
load-time theme cascade, minimal font packs, automation IPC + thin MCP.

## Parts

| Part | Covers |
| ---- | ------ |
| [01 — Module System](01-module-system.md) | Vocabulary, `module.json`, kinds, `uses`/`provides`/`implements`, interfaces, providers, root graph, capabilities, lifecycle, trust |
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

One mental model repeats everywhere:

1. **Semantic names, not concrete assets.** Templates use logical icon names,
   font roles, theme tokens, and i18n keys; packs map them to real assets.
2. **Ordered chains with fallback** for multi-active resources (icons, fonts,
   language packs); **winner-takes-all** for coherence-critical ones (theme).
3. **Declared defaults, sparse user overrides.** Modules and packs declare
   defaults; the settings store holds only what the user changed.
4. **More specific wins.** Author default → user global → author instance →
   user per-instance, everywhere a value can be layered.
5. **The core wires, modules work.** Rust routes generic records; behavior
   lives in Luau modules and declarative contracts.

## Related reference docs (not part of this spec)

- [`../llm-context.md`](../llm-context.md) — codebase orientation (crates, data flows).
- [`../frontend/mesh-syntax.md`](../frontend/mesh-syntax.md) — `.mesh` syntax reference.
- [`../frontend/elements.md`](../frontend/elements.md) — native element taxonomy.
- [`../modules/README.md`](../modules/README.md) — shipped module index.
