# `@mesh/navigation-bar`

Top-edge navigation surface.

- **Type:** `surface`
- **Entrypoint:** `src/main.mesh`

## What it demonstrates

- A compact, theme-token-driven top navigation bar
- Displaying audio backend state through shell-injected frontend data
- Surface placement through `config/settings.json`
- Container-query adaptation for narrower widths

## Default behavior

The shell discovers this as its own top-level frontend surface, and the
default settings pin it to the top edge with an exclusive zone so it behaves
like normal shell chrome.
