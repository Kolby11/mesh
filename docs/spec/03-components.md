# 03 — Components & Props

> Part of the [MESH Specification](README.md). Syntax reference:
> [`../frontend/mesh-syntax.md`](../frontend/mesh-syntax.md), element taxonomy:
> [`../frontend/elements.md`](../frontend/elements.md).

A **component** is a user-authored reusable `.mesh` unit composed from core
elements and other components. A **frontend module** packages one or more
components into a complete shell feature. Configuration belongs to the
component's source as a typed `<props>` public API; packaging stays in the
manifest.

## 1. The `.mesh` file

```
<props>                 typed, defaulted, localized configuration (§3)
<template>              XHTML-like markup: elements, {expressions}, components
<script lang="luau">    state, lifecycle, handlers (real Luau VM)
<style>                 CSS-like styling: var(--…) tokens, prop(…) references
```

**Status: shipped** except `<props>` (**target**, §3).

Component rules (shipped):

- Keep files small; extract list items and distinct UI blocks into components
  under `components/`, imported with
  `local ItemRow = require("./components/item-row.mesh")`.
- Bare non-local script assignments are public reactive members; templates
  bind `{name}`. `local` is private. Hooks receive `self` (`self.meta`,
  `self.storage`).
- `bind:this={ref}` is a **live reference** into the child's environment (all
  components of one surface share a single Lua realm, each in its own
  `_ENV`): reads see current values, calls are synchronous, and named event
  channels cross the boundary (`ref.Changed:on(fn)`). Host internals and
  lifecycle hooks stay private.
- `refs.<name>` is the element-node analog: last-painted geometry/state reads
  plus imperative actions (`focus`, `blur`, `click`, `scroll_into_view`,
  `scroll_to`, `set_value`) routed through the real interaction paths.
- Service data comes from interface proxies (`require("mesh.audio@>=1.0")`);
  display state (icon names, labels) is computed in the component's own
  script — never injected by the core.

### 1.1 Popovers and escape-bounds UI

**Status: shipped.** Surfaces are containers, not authoring units. An in-tree
`<popover open>` is authored inline and *deterministically* promoted to a
compositor `xdg_popup` child surface when shown (content-driven size,
compositor anchoring, grab/hover-bridge dismiss owned by the core). Small
menus are embeddable `component`-kind modules with no surface geometry; only
true top-level surfaces own `mesh.surface` placement.

## 2. Sizing — CSS is the engine

**Status: shipped.** Every surface and component is sized by CSS measurement
of its root box: `width: 100%` spans, fixed lengths pin, `fit-content`
shrinks, `min-*`/`max-*` clamp. Manifest sizing fields do not exist. The
show/hide transition is CSS on the root. Script-side calculation reads
measured geometry from `refs.*`, computes a **value**, assigns it to a prop;
CSS re-consumes it next frame — one shared named value, no geometry fight.
Imperative `refs` geometry writes are the documented last-writer-wins escape
hatch, not the default path.

## 3. The `<props>` block

**Status: target** (design final; Phase 3 manifest narrowing already landed —
see §7).

One declaration per configurable value; everything else derives from it.

```html
<props>
  width:    { type: "size",  default: "fit-content", label: t("var.width") }
  density:  { type: "enum",  options: ["compact", "cozy"], default: "cozy",
              label: t("var.density") }
  show_pct: { type: "bool",  default: true, label: t("var.show_percent") }
  gap:      { type: "size",  default: "var(--spacing-xs)" }
  accent:   { type: "token", default: "color-primary" }
  icon:     { type: "icon",  default: "audio-volume-high" }
  anim_ms:  { type: "duration", default: 120, min: 0, max: 1000 }
</props>
```

Fields: `type` (required), `default`, `label`/`description` (`LocalizedText`,
resolved through the module's i18n catalogs — [07](07-i18n.md)), `options`
(enum), `min`/`max`/`step`/`unit`, `expose` (default `true`; `false` =
CSS/script-only internal knob, no settings row).

### 3.1 Three projections

```
<props> entry ──► prop(name) in <style>     (typed, component-scoped CSS reference)
             ──► props.name in <script>     (reactive read/write)
             ──► generated settings row     (typed control + i18n label → settings store)
```

`prop()` is deliberately **not** `var(--…)`: prop names are component-scoped
(two components can both declare `width` with zero interaction), and `prop()`
is a typed reference checked at the use site — `width: prop(density)` is a
compile error. `prop(name)` is usable anywhere `var()` is, including inside
`calc()` and shorthand positions; substitution happens with `var()` in the
same pass, before `calc()` evaluation, and each position type-checks against
its sub-property's domain.

### 3.2 Types

| Type | CSS yields | Lua | Generated control |
| ---- | ---------- | --- | ----------------- |
| `size` | length / `%` / sizing keyword | string/number | length input |
| `number` / `int` | unit-suffixed number | number | numeric / slider |
| `bool` | `0` / `1` | boolean | toggle |
| `enum` | chosen string | string | dropdown / segmented |
| `string` | string | string | text field |
| `color` | color literal | string | color picker |
| `token` | resolved theme token | string | theme-aware token picker |
| `duration` | `<n>ms` | number (ms) | numeric |
| `icon` | logical icon name | string | pack-aware icon picker |

`token` binds config into theming ([04](04-styling.md)); `icon` binds into
icon packs ([05](05-icons.md)): defaults and overrides must resolve against
installed packs (compile diagnostic otherwise), and declared icon props feed
the module's `iconRequirements`.

Validation is enforced three times with one value grammar (the CSS value
parser, not a second parser): compile time (defaults, settings overrides,
instance attributes), authoring time (LSP completes type-scoped values,
`props.<name>`, `prop(<name>)`; hover shows type/default/label), and runtime
(typed settings controls; script writes coerced/validated before reaching
CSS).

## 4. Precedence — one specificity ladder

Every prop resolves to **one value** before `prop()`/`props.name` reads it.
Lowest → highest:

1. **Author default** — `<props>` `default`.
2. **User global setting** — "all mixers compact"; the primary user knob.
3. **Author instance prop** — `<VolumeMixer width="320px"/>`; protects the
   layout the author built here.
4. **User per-instance setting** — this exact placement; the user is never
   trapped by an author instance value.
5. **Script assignment** — `props.width = computed` (reactive).
6. **Imperative `refs` geometry write** — advanced, last-writer-wins.

Same model as CSS and layered config: *more specific wins*. Script-only layer
introspection: `props.source(name)` → winning layer;
`props.at(name, scope)` → raw value at
`"default" | "global" | "instance" | "per_instance" | "script"` or `nil`.

Storage shape and namespace rules live in [08 — Settings](08-settings.md);
per-instance keys reuse the existing composition instance key
(`{host_instance_key}/import:{alias}`), and root-graph instances
([01 §5.1](01-module-system.md)) prefix it with `module-id#instance-id`.

## 5. Props everywhere (non-`.mesh` modules)

**Status: target.** The same grammar covers the whole system, so settings UI
generation has one input model:

- **Backends** open `main.luau` with a statically parseable block, Luau
  syntax, same semantics:

  ```luau
  props {
    poll_interval = { type = "duration", default = 500, min = 100 },
    device_filter = { type = "string", default = "" },
  }
  ```

- **Interfaces** declare provider-swap-surviving shared props in the
  contract JSON inside `module.json` (`contract.props`); every implementation
  reads them under the contract's settings namespace ([08 §4](08-settings.md)).
- **Shell core** declares its own knobs (tooltip timing, activation keys, …)
  with the same declaration model, generating the shell settings pages.

This replaces the `mesh.provides.settings` schema boilerplate everywhere.

## 6. Worked example

```html
<props>
  track_width:  { type: "size", default: "20px",  label: t("var.track_width") }
  track_height: { type: "size", default: "100px", label: t("var.track_height") }
  anim_ms:      { type: "duration", default: 120, min: 0, max: 600 }
</props>

<template>
  <popover class="audio-popover-shell" aria-label="Audio controls">
    <column class="audio-popover">
      <slider class="audio-slider" min="0" max="100" value="{slider_value}"
              orient="vertical" onchange={onVolumeChange}/>
      <text class="audio-percent">{audio_percent_label}</text>
    </column>
  </popover>
</template>

<style>
.audio-popover-shell {
  min-width: 36px; max-width: 48px;   /* content-measured, clamped */
  transition: opacity prop(anim_ms) var(--animation-curves-bezier-standard);
}
.audio-slider { width: prop(track_width); height: prop(track_height); }
</style>
```

No manifest geometry; the user retunes track size and animation through
generated settings.

## 7. Implementation roadmap

1. **Phase 1** — `<props>` parsing + all three projections for the `size`
   type end-to-end (parser block, `StyleValue::Prop`, precedence resolver,
   reactive `props` table, derived settings schema).
2. **Phase 2** — migrate `@mesh/audio-popover` as the reference proof.
3. **Phase 3 — landed.** Manifest narrowed to placement-only
   `SurfaceLayoutSection`; sizing/`display_transition`/`mesh.settings`
   deleted with no compat readers; LSP manifest schema synced.
4. **Phase 4** — remaining types, per-instance persistence in the settings
   store, LSP completions/hover/diagnostics, then in-script backend props and
   interface props (§5).

No phase leaves an old config path readable (no-backward-compat rule).
