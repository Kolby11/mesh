# MESH Documentation

Docs for the MESH shell framework. For a high-level project description see
[`../README.md`](../README.md); for the authoritative module / capability /
lifecycle spec see [`../spec/pluggable-backend.md`](../spec/pluggable-backend.md).

## Contents

- **[`module-system.md`](./module-system.md)** — target package/module model,
  including npm-compatible `package.json` + `mesh`, backend/frontend workflow,
  interface contracts, and shared Luau library modules.
- **[`extensibility.md`](./extensibility.md)** — interface registry, contract
  packages, cross-language bindings, event channels, capability
  classification. The backbone of the extensibility story.
- **Modules**
  - [`modules/README.md`](./modules/README.md) — core-module index.
  - [shipped frontend module docs](./modules/frontend/core/README.md) — shipped
    frontend surfaces and widgets.
  - [example frontend module docs](./modules/frontend/examples/README.md) —
    example frontend compositions and reusable sample widgets.
  - [shipped backend module docs](./modules/backend/core/README.md) — shipped
    backends and interface packages.
- **Frontend**
  - [`frontend/mesh-syntax.md`](./frontend/mesh-syntax.md) — `.mesh` component syntax: tags, text interpolation, attribute binding, two-way binding, event handlers, accessibility.
  - [`frontend/html-css-transition.md`](./frontend/html-css-transition.md) — transition sketch for a Qt-style UI XML vocabulary and bounded CSS profile over the current MESH UI/runtime pipeline.
  - [`frontend/slots.md`](./frontend/slots.md) — slot points for extending
    surfaces without forking them.
- **Performance**
  - [`performance-roadmap.md`](./performance-roadmap.md) — retained rendering,
    dirty invalidation, damage tracking, text/glyph caching, and GPU sequencing
    needed to approach Qt-like performance.
- **Theming & localization**
  - [`theming/themes.md`](./theming/themes.md) — theme tokens, component
    defaults, module-owned theme subtrees, modes, hot-swap.
  - [`theming/icons.md`](./theming/icons.md) — icon packs, fallback chain,
    Material-3-style variable axes.
  - [`theming/locales.md`](./theming/locales.md) — module-bundled
    translations, third-party language packs, locale fallback chain.
- **Settings**
  - [`settings/README.md`](./settings/README.md) — JSON settings with a
    six-layer override stack, contract-level shared schemas.
- **Lifecycle**
  - [`installation.md`](./installation.md) — `package.json` manifest,
    dependency kinds, resolution, multi-provider handling, lockfile.
  - [`health.md`](./health.md) — module health states, dep-driven fix
    suggestions, health subscriptions, `mesh doctor`.
