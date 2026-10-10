# Authoring model: plain Luau with little overhead

**Status:** review, 2026-10-10. Nothing implemented. Backlog lines are in
"Authoring model" in [`docs/BACKLOG.md`](../../../docs/BACKLOG.md). Timers,
typed shell calls and hover intent are in
[the scheduler note](2026-10-10-script-timers-and-typed-shell-ops.md).

## Goal

A component author should write ordinary Luau: plain data in script state,
presentation in the template, and service calls as ordinary function calls.
Dependency tracking, validation, scheduling and diffing belong to the
framework. The same changes also cut runtime work, because less derived state
crosses the Lua→JSON boundary and fewer scripts re-run per update.

## Evidence from shipped modules

Counted over `modules/frontend` (about 9k lines of `.mesh`) on 2026-10-10:

| Pattern | Count | Missing primitive |
| --- | --- | --- |
| `xxx_class = "a a-active"` assignments | 96 | class/style directives |
| `tonumber(` / `type(x) == "function"` / inline `math.max(…math.min` clamps | 39 / 20 / 12 | typed service payloads |
| Files with a hand-written clamp/normalize | 6 | typed payloads, shared libraries |
| List rows carrying `button_class`, `check_hidden`, `"background: " .. color` | most settings pages | directives, `{#if}` |
| Copies of the volume-icon thresholds | 3 | shared Luau libraries |
| Copies of the hover close bridge | 6 (~40 lines each) | timers, hover intent |
| `pcall(require, "mesh.<svc>")` for optional services | several | declared optional dependencies |
| `self.X:fire(e)` forwarding handlers; `init` capturing `self.Changed` | throughout | event forwarding, top-level `self` |
| Service calls through both proxy methods and `mesh.events.publish("mesh.<svc>.<method>")` | both in use | one call path |

## Mechanics

### `render(self)` is a coarse recompute hook

`render()` runs on mount, theme/locale change, reload, and any service update
that dirties the surface. When it runs, it runs for every runtime on that
surface (`ShellComponent::call_render_hooks`), whichever service changed.
Authors derive everything there imperatively, so each poll re-executes every
instance's derivation, rebuilds row tables and strings, and converts them to
JSON for the template diff.

The service proxies already record which fields each instance reads. The first
step is to re-run `render` only for instances whose tracked reads changed. The
fuller step is derived values (`computed`) and watchers
(`mesh.watch(proxy, field, fn)`) that re-run only on their own dependencies.

### Presentation in state

The template has `{expr}`, `{#for … key=}`, `hidden=` and bindings, but no
class/style directives and no conditional block. Scripts therefore precompute
class strings, inline style strings and `hidden` flags for every node and row.
Add `class:name={cond}`, `style:prop={value}` and `{#if}/{:else if}/{:else}`.
Document `{#for}`, which ships but is missing from `docs/frontend/mesh-syntax.md`.

### Untrusted payloads

Proxies expose raw provider JSON, so every consumer re-validates it. Contracts
should be the source of truth (S03-DEAD-001 compiled contracts): the proxy
delivers typed, range-checked fields with documented defaults, and an absent
optional service is a defined `nil`/`available = false`, not a `pcall`.

### Optimistic commands

Sliders set local state, call the method, and then reconcile with the next
emitted state. The proxy could hold a pending value per field until the
provider confirms or rejects it, using the contract's coalescing keys (RT-09).

## Not decided here

- Whether directives lower to the existing attribute pipeline or to typed
  node fields. This depends on the typed `WidgetNode` work.
- Whether `computed` values are Rust-owned reactive values (backlog item "R")
  or Luau closures tracked by the proxies.
- Library sharing and its capability boundary (01 §capabilities).
