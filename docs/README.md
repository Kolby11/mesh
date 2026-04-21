# MESH Documentation

Docs for the MESH shell framework. For a high-level project description see
[`../README.md`](../README.md); for the authoritative plugin / capability /
lifecycle spec see [`../spec/pluggable-backend.md`](../spec/pluggable-backend.md).

## Contents

- **[`extensibility.md`](./extensibility.md)** — interface registry, contract
  packages, cross-language bindings, event channels, capability
  classification. The backbone of the extensibility story.
- **Plugins**
  - [`plugins/README.md`](./plugins/README.md) — core-plugin index.
  - [`plugins/frontend/core/`](./plugins/frontend/core/README.md) — shipped
    frontend surfaces and widgets.
  - [`plugins/frontend/examples/`](./plugins/frontend/examples/README.md) —
    example frontend compositions and reusable sample widgets.
  - [`plugins/backend/core/`](./plugins/backend/core/README.md) — shipped
    backends and interface packages.
- **Frontend**
  - [`frontend/slots.md`](./frontend/slots.md) — slot points for extending
    surfaces without forking them.
- **Theming & localization**
  - [`theming/themes.md`](./theming/themes.md) — token-based theming with
    inheritance, modes, hot-swap.
  - [`theming/icons.md`](./theming/icons.md) — icon packs, fallback chain,
    Material-3-style variable axes.
  - [`theming/locales.md`](./theming/locales.md) — plugin-bundled
    translations, third-party language packs, locale fallback chain.
- **Settings**
  - [`settings/README.md`](./settings/README.md) — JSON settings with a
    six-layer override stack, contract-level shared schemas.
- **Lifecycle**
  - [`installation.md`](./installation.md) — `plugin.json` manifest,
    dependency kinds, resolution, multi-provider handling, lockfile.
  - [`health.md`](./health.md) — plugin health states, dep-driven fix
    suggestions, `widget-fallback`, `mesh doctor`.
