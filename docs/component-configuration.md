# Component Configuration Model

> Status: **design spec** (not yet implemented). This document defines the target
> authoring model for configurable, reusable MESH components. It supersedes the
> scattered `mesh.surface` sizing + `mesh.settings` schema approach.
>
> **No backward compatibility.** When implemented, the old config surfaces are
> *removed*, not kept alongside the new one — there are no compat shims and no
> "migration-compat" inputs. The `mesh.surface` sizing fields and the standalone
> `mesh.settings` schema are deleted, and any code that reads them is deleted in
> the same change.

---

## 1. Motivation — the four-surface problem

Today a single conceptual value (say, "popover width") can be declared and kept in
sync across **four** authoring surfaces:

- **`module.json` `mesh.surface`** — placement *and* sizing mixed together
  (`anchor`, `layer`, `width`, `height`, `size_policy`, `min_*`/`max_*`,
  `display_transition`). See `SurfaceLayoutSection` in
  `crates/core/extension/module/src/manifest/model.rs` and
  `surface_layout_from_manifest()` in `crates/core/surface-config/src/lib.rs`.
  This forces even small **embeddable** components (e.g. `@mesh/audio-popover`) to
  describe themselves like top-level Wayland surfaces with fixed pixel sizes.
- **`module.json` `mesh.settings`** — a separate typed schema for persisted user
  settings (`SettingsSection`, `model.rs`).
- **`<style>`** — CSS `var(--…)` values (resolved via `VariableStore`).
- **`<script>`** — logic that reads `{settings.*}` injected as Lua state, plus
  template attributes injected as script variables (`runtime.rs`).

An author must learn and synchronize all four. The result is duplication, drift,
and a high cost of entry for what should be a simple "let the user customize this."

**Goal:** collapse configuration into **one declaration** that auto-projects to
CSS, script, and the settings UI; keep **CSS as the primary sizing engine** with a
script override hatch; and stop making embeddable components declare surface
geometry. This formalizes a direction the codebase already leans toward — content
is sized by CSS measurement (see `docs/llm-context.md`, the popover-promotion
work, and the "CSS drives surface size" behavior).

---

## 2. The `<props>` block

A component declares its configurable surface **once**, in a new `<props>` block
inside its single `.mesh` file — alongside `<template>`, `<script>`, and `<style>`.

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

Each entry is a typed, defaulted, optionally localized variable. **`<props>` is the
component's public, typed API — like function parameters.**

### Field grammar

| Field          | Meaning |
| -------------- | ------- |
| `type`         | The validated value domain (see §2.2). Required. |
| `default`      | Baseline value. May reference a theme `var(--…)` / token. |
| `label`        | `LocalizedText` (`{ t, fallback }` or a literal) for the settings UI. |
| `description`  | `LocalizedText` long-form help for the settings UI. |
| `options`      | Allowed values for `enum`. |
| `min`/`max`/`step` | Numeric/size/duration constraints. |
| `unit`         | Default unit for numeric/size values. |
| `expose`       | `true` (default) shows it in the generated settings UI; `false` makes it a CSS/script-only internal knob. |

`label`/`description` reuse the existing `LocalizedText` enum (`model.rs`) and the
`LocaleEngine` localization flow — no new i18n surface.

### 2.1. Three projections (one declaration → three consumers)

```
            ┌──────────────────────── <props> (single declaration) ────────────────────────┐
            │  width:   { type: size, default: "fit-content", label: t("var.width") }       │
            │  density: { type: enum, options: ["compact","cozy"], default: "cozy" }        │
            └──────────────────────────────────────────────────────────────────────────────┘
                 │ auto-project             │ auto-project              │ auto-project
                 ▼                          ▼                           ▼
        typed CSS reference          reactive Lua table          generated settings UI
        prop(width) in <style>       props.width (tracked r/w)   typed row + i18n label,
        (component-scoped,           reads reactive,             persisted to settings.json
         type-checked use site)      writes → prop               (global + per-instance)
```

The author never writes a CSS var name, a settings schema, or a script binding by
hand — all three fall out of the declaration.

#### `prop(name)` is its own namespace — not bare `var(--…)`

Props are referenced in `<style>` via a dedicated **`prop(name)`** function and in
script via **`props.name`** — deliberately *not* the flat `var(--…)` space theme
tokens live in. This is what keeps CSS and scripting from colliding, and keeps
names readable:

- A prop named `width` can never be confused with the CSS `width` property or an
  inherited `--…` theme token.
- Two different components can each declare `width` with **zero interaction** —
  prop names are component-scoped, not globally cascaded.
- Unlike `var()` (opaque text substitution the engine can't type-check), `prop()`
  is a **typed reference resolved at the use site**. The compiler knows
  `prop(width)` yields a `size`, so `height: prop(label)` (a string used as a
  length) or `width: prop(density)` is a *compile error*.

`prop()` is implemented alongside `var()` in
`crates/core/ui/component/src/style.rs`.

```css
/* clearly a component input — not the CSS width, not a theme token */
.mixer { width: prop(width); gap: prop(gap); }
```
```lua
-- same value, read/written from script
props.density = "compact"
```

### 2.2. Variable types

| Type           | CSS reference yields          | Lua value     | Settings UI control      |
| -------------- | ----------------------------- | ------------- | ------------------------ |
| `size`         | length / `%` / sizing keyword | string/number | length input + unit/keyword |
| `number`/`int` | unit-suffixed number          | number        | numeric / slider         |
| `bool`         | `0` / `1`                     | boolean       | toggle                   |
| `enum`         | the chosen string             | string        | segmented / dropdown     |
| `string`       | string                        | string        | text field               |
| `color`        | color literal                 | string        | color picker             |
| `token`        | the resolved theme token      | string        | token picker (theme-aware) |
| `duration`     | `<n>ms`                       | number (ms)   | numeric                  |
| `icon`         | (logical icon name)           | icon name     | icon picker (pack-aware) |

`token` and `icon` tie configuration into **theming** and **icon packs** without
hardcoding — a component can expose a swappable accent token or icon as user
config, validated against installed packs (see §7).

### 2.3. Typed validation & IntelliSense

The `type` is a **validated value domain**, enforced at three points, so a field
declared for width can only ever hold a CSS width value:

1. **Compile-time** — every `default`, user-settings override, and instance-prop
   attribute is parsed/validated against its declared type. A `size` must be a
   valid CSS length, percentage, or sizing keyword (`fit-content`, `auto`,
   `min-content`, `max-content`, `var(--…)`); a `duration` must be a number or
   `<n>ms`; an `enum` must be one of `options`; a `token` must resolve to a known
   theme token; an `icon` must resolve in the icon-pack chain. Invalid values are a
   diagnostic, not a silent fallback. This reuses the CSS value parsing already in
   `crates/core/ui/component/src/style.rs` — not a second parser.
2. **LSP (authoring IntelliSense)** — the LSP reads `<props>` and offers
   completions *scoped to the field's type*: inside a `size` value it completes CSS
   length units and sizing keywords (and flags non-length values); inside `enum` it
   completes the declared `options`; inside `icon`/`token` it completes installed
   icon/token names. The same declaration also drives `props.<name>` script
   completion and `prop(<name>)` style completion. Hover shows type, default, and
   i18n label. Extends the existing manifest + script/CSS analyzers.
3. **Runtime** — generated settings controls are type-specific (a `size` row only
   accepts CSS sizes; an `enum` is a closed dropdown), and a script write
   `props.width = x` is coerced/validated to the declared type before it reaches
   CSS — script can't smuggle an invalid value into the cascade.

Net effect: `width: { type: "size" }` means *width only ever accepts CSS width
values* — in the `.mesh`, in user settings, in the editor, and from script.

---

## 3. Precedence — one specificity ladder ("more specific wins")

Every prop resolves to **one value**, computed by the runtime *before* `prop()` /
`props.name` ever reads it. Authors and CSS never merge layers by hand, and there
is never more than one reference function — `prop(name)` always returns the
resolved value. Ordering, lowest → highest:

1. **Author default** — the component's built-in baseline (`<props>` `default`).
2. **User global setting** — the user's broad, everyday customization for the
   component ("all mixers compact"). The *primary* user knob; beats the author's
   generic default.
3. **Author instance prop** — the author embedding the component *here* with a
   specific value: `<VolumeMixer width="320px"/>`. More specific than a global
   stroke, so it sits above the global setting and protects the layout the author
   built. Reuses today's prop path (template attrs → `props_json` → script state,
   `composition.rs`, `runtime.rs`).
4. **User per-instance setting** — the user customizing *this exact* placement. The
   most specific user intent, so the user can always override an author's instance
   value *at the matching specificity level* without globally steamrolling author
   intent. User settings therefore exist at **two scopes** — global and
   per-instance, keyed by component namespace + instance id.
5. **Script assignment** — `props.width = computed`. Reactive: updates the resolved
   value and triggers re-render.
6. **Imperative geometry override** (advanced, last-writer-wins) — a script may
   imperatively set an element's geometry via a `refs.<name>` style write, acting
   like an inline style at the top of the cascade. Documented escape hatch, not the
   default path.

**Why this ordering is natural:** it's the same model as CSS (inline beats
stylesheet) and layered config (local beats global) — nobody memorizes a special
rule; "more specific wins" covers every case. The user's primary lever (global
settings) sits low and broad; an author's deliberate per-placement value is more
specific and sits above it; the user is never trapped because the per-instance
setting is more specific still. The per-instance user scope is exactly what lets
this be a clean ladder instead of an arbitrary "instance vs setting" tiebreak.

> **Advanced layer access (script only).** The common path always reads the single
> resolved value via `prop(name)` / `props.name`. In the rare case a script needs a
> *specific* layer (e.g. "what did the user save, ignoring the instance override?"),
> two read-only helpers are exposed in script only: `props.source(name)` returns the
> winning layer (`"default"` | `"global"` | `"instance"` | `"per_instance"` |
> `"script"`), and `props.at(name, scope)` returns the raw value at a given layer or
> `nil`. These are explicit functions, not magic tables — CSS and the everyday path
> never touch layers. (See §9 for why this shape was chosen.)

---

## 4. Sizing & the CSS-vs-script boundary

**CSS is the primary sizing engine.** A component that omits a size prop defaults to
intrinsic/content sizing (`fit-content`) — matching the existing "CSS drives
surface size" behavior and the popover content-measure path.

The non-collision guarantee comes from a **single shared, named value**:

- CSS reads it via `prop(width)`.
- Script reads/writes the *same* value via `props.width`.
- They never independently own the same property — they share one prop, so there is
  no silent fight. Script-side calculation works cleanly: the script reads measured
  geometry from `refs.<name>` (read-only, last-painted frame), computes a **value**,
  and assigns it to a prop; CSS re-consumes it next frame.

```lua
-- script calc feeding CSS, no geometry fight
function render(self)
  props.columns = math.max(1, math.floor(refs.grid.width / 64))
end
```
```css
.grid { grid-template-columns: repeat(prop(columns), 1fr); width: prop(width); }
```

**Script may override geometry (last-writer-wins).** Beyond the shared-prop path, a
script *may* imperatively set an element's width/height via `refs.<name>` (an
inline-style equivalent at the top of the cascade). This is the power escape hatch;
it is last-writer-wins over CSS and documented as advanced. The recommended path
stays: assign a prop, let CSS lay out.

---

## 5. Surface block reduction (embeddables stop declaring geometry)

`mesh.surface` is narrowed to **true top-level Wayland placement only**: `anchor`,
`layer`, `exclusive_zone`, `keyboard_mode`, `visible_on_start`, `margins`. Sizing
(`width`, `height`, `size_policy`, `min_*`/`max_*`,
`prefers_content_children_sizing`, `display_transition`) is **removed** from the
manifest and derived from the component's CSS + `<props>`:

- **Embeddable components & popovers** declare *no* surface block at all — they are
  sized by CSS content measurement (the popover-promotion model).
- **Top-level layer-shell surfaces** that need a reserved size (e.g. the bar's
  `exclusive_zone` height) keep `anchor`/`layer`/`exclusive_zone`, but express the
  visual size as CSS/props; `exclusive_zone` reads the measured/declared size.

`SurfaceLayoutSection` shrinks accordingly; `surface-config` resolves placement from
the manifest and size from measurement. **The removed fields are deleted, not
deprecated** — `width`, `height`, `size_policy`, `min_*`/`max_*`,
`prefers_content_children_sizing`, and `display_transition` come out of
`SurfaceLayoutSection` and `surface_layout_from_manifest()`, and the standalone
`mesh.settings` `SettingsSection` is removed in favor of the `<props>`-derived
schema. No code continues to read the old shapes.

---

## 6. Subsystem integration

### i18n
Prop `label`/`description` and `enum` option labels are `LocalizedText`; the
generated settings UI resolves them through `LocaleEngine` + the module's
`provides.i18n`. No new i18n surface.

### Icon packs (first-class via `type: "icon"`)
- **Resolution** — an icon value is a *logical icon name* resolved through the
  component's declared icon-pack chain (`uses.resources.icons`, e.g.
  `@mesh/icons-default`) and the XDG/icon resolver — never a hardcoded path.
- **Validation** — at compile time the default and any override must resolve in an
  installed pack; unresolved names are a diagnostic. Names also feed the module's
  `iconRequirements` so packaging knows which glyphs the component needs.
- **Settings UI** — the generated control is an **icon picker** populated from the
  active icon pack(s), letting a user re-skin a component's icon as config.
- **Reactive** — an icon value is readable as `props.icon` in script; changing it
  re-renders the `<icon>` element through the normal icon pipeline.
- **Theming-aware** — icon resolution respects the active icon-pack/theme, so the
  component follows the system icon theme without author changes.

### Theme tokens (`type: "token"`)
Binds config to the theme system: a component exposes a controlled accent/spacing
knob that still inherits the global token language. Global look stays in theme
tokens; per-component/instance knobs live in props.

### Backend communication (distinct, but reads identically)
Live system state stays the interface proxy (`require("mesh.audio")`); config is
`props`. Both are **reactive tables read the same way** (`audio.percent`,
`props.width`), so authors learn one access pattern. The rule, stated plainly:
*service proxies = live system state; props = configuration/customization.*

### LSP / DX
Completions for `props.<name>` (from `<props>`), `prop(<name>)` in `<style>`,
type-scoped completion of each field's value, and settings-schema validation —
reusing the existing LSP manifest + script analyzers.

---

## 7. What this buys the author

The mental model collapses from "four config surfaces to keep in sync" to **one
small declarative block** everything else derives from:

- `module.json` shrinks to packaging only — identity, `kind`, capabilities,
  dependencies, interfaces, i18n/icon provides. **No per-field config duplication.**
- `<props>` is not a "fourth language to memorize" — it is a tiny typed schema
  whose entries auto-appear as `prop()` references, `props.*` Lua fields, and
  settings rows.
- Components become genuinely **standalone & reusable**: a component carries its
  own configurable contract, works embedded (instance props) or top-level (surface
  placement), and customizes via CSS-first sizing with a script override hatch.

---

## 8. Worked example — migrating `@mesh/audio-popover`

**Today** the popover is described as a Wayland surface in `module.json`:

```json
"surface": {
  "anchor": "left", "layer": "overlay",
  "width": 40, "height": 200,
  "size": "content_measured", "prefers_content_children_sizing": true,
  "min_width": 36, "max_width": 48, "min_height": 180, "max_height": 260,
  "display_transition": { "show_ms": 120, "hide_ms": 120 }
}
```

**Under this model** the popover is an embeddable component: it declares *no*
surface geometry, sizes itself from CSS, and exposes any tunables as props.

```html
<props>
  track_width:  { type: "size", default: "20px", label: t("var.track_width") }
  track_height: { type: "size", default: "100px", label: t("var.track_height") }
  anim_ms:      { type: "duration", default: 120, min: 0, max: 600 }
</props>

<template>
  <popover class="audio-popover-shell" aria-label="Audio controls">
    <column class="audio-popover">
      <slider class="audio-slider" min="0" max="100" value="{slider_value}"
              orient="vertical" onchange={onVolumeChange} onrelease={onVolumeRelease}/>
      <text class="audio-percent">{audio_percent_label}</text>
    </column>
  </popover>
</template>

<style>
.audio-popover-shell {
  /* no width/height — content-measured by CSS, clamps via min/max */
  min-width: 36px; max-width: 48px;
  padding: 0 var(--spacing-sm) var(--spacing-sm) var(--spacing-sm);
  transition: opacity prop(anim_ms) var(--animation-curves-bezier-standard);
}
.audio-slider { width: prop(track_width); height: prop(track_height); }
</style>
```

The fixed `40×200` numbers, `size_policy`, the `min_*`/`max_*` clamps, and
`display_transition` all move out of `module.json` and into CSS + `<props>`. No
capability is lost: the popover still content-measures, still clamps, still
animates show/hide; and the user can now retune the track size or animation through
generated settings without touching the manifest.

---

## 9. Resolved design decisions

These were open during design and are now decided, so implementation has no
ambiguity to relitigate.

### 9.1 Per-instance settings storage & keying

`config/settings.json` (per module dir) carries **two prop scopes** under a single
`props` object:

```json
{
  "props": {
    "global":   { "track_width": "24px", "anim_ms": 90 },
    "instances": {
      "@mesh/navigation-bar/import:audio": { "track_width": "28px" }
    }
  }
}
```

- **global** = applies to every instance of this component (the user's primary
  knob).
- **instances** = keyed by the **existing composition instance key** already
  computed in `crates/core/shell/src/shell/component/composition.rs`
  (`{host_instance_key}/import:{alias}`, see the `instance_key` used at
  `composition.rs:96`). Reusing it keeps keys stable and deterministic — no new id
  scheme. The settings UI writes the active surface's instance key when the user
  edits "this instance only".

### 9.2 `prop()` inside `calc()` and shorthands

`prop(name)` is a value-level token usable **anywhere `var()` is**, including inside
`calc()` and in shorthand positions (`padding: prop(gap) prop(gap)`). Resolution
order: `prop()` and `var()` are substituted to their concrete literals in the
**same pass, before** `calc()` evaluation — so `calc()` always sees concrete
values and needs no special prop handling. Type-checking composes by position: a
prop in a length context (a `calc()` term or a length sub-slot of a shorthand) must
be length-compatible (`size`/`number`/`duration`); each shorthand sub-position is
checked against that sub-property's own domain. A type mismatch is a compile
diagnostic at the use site.

### 9.3 Script-only per-layer read API

Minimal and explicit — **two read-only functions**, no magic tables:
`props.source(name)` → the winning layer string; `props.at(name, scope)` → the raw
value at a layer (`"default"`/`"global"`/`"instance"`/`"per_instance"`/`"script"`)
or `nil`. The common path stays `prop(name)` / `props.name` = the single resolved
value. Chosen over `props.setting.*`/`props.passed.*` because two named functions
are easier to type-complete in the LSP and read unambiguously.

---

## 10. Implementation plan (phased, with files)

Each phase is independently shippable and ends green (`nix develop -c cargo test`).
Build per `project_animation_engine` note: `nix develop`.

### Phase 1 — `<props>` parsing + the three projections for the `size` type

Goal: a component can declare `width: { type: "size", default: "fit-content" }`,
reference it as `prop(width)` in CSS and `props.width` in script (reactive), and a
generated settings schema reflects it — end-to-end for `size` only.

- **Parse the block** — `crates/core/ui/component/src/lib.rs`: add
  `pub props: Option<PropsBlock>` to `ComponentFile` (`lib.rs:45`); define
  `PropsBlock`, `PropDef { name, ty, default, label, description, constraints,
  expose }`, and a `PropType` enum. New submodule
  `crates/core/ui/component/src/parser/props.rs`; register `"props"` in
  `extract_blocks` (`parser.rs`) and parse entries (unknown block already handled
  by `ParseError::UnknownBlock`).
- **`prop()` CSS token** — `crates/core/ui/component/src/style.rs`: add
  `StyleValue::Prop(String)` beside `Var` (`style.rs:153`). Recognize `prop(name)`
  in `crates/core/ui/component/src/parser/styles.rs` (mirror the `var(` path).
- **Resolve `prop()`** — resolve `StyleValue::Prop` against a per-instance prop
  value map in the style resolver: `crates/core/ui/elements/src/style.rs`
  (`StyleResolver`) and the compiler resolution path in
  `crates/core/frontend/compiler/src/render.rs` / `style.rs`. The value map is
  produced by the new resolver in the next bullet.
- **Precedence resolver** — new `crates/core/shell/src/shell/component/props.rs`:
  computes the resolved value per prop from default (PropsBlock) → global setting →
  instance prop (composition attrs) → per-instance setting → script. Feeds both the
  style value map and the script `props` table.
- **Reactive `props` Lua table** — `crates/core/runtime/scripting/src/context/`
  (add a `props` proxy modeled on the interface proxy / `element_ref.rs`
  reactivity): reads tracked like service fields; writes update the value map, mark
  dirty, and re-resolve. Wire it where props are currently injected as flat state in
  `crates/core/shell/src/shell/component/runtime.rs:177` (and `:248`) — replacing the
  flat `state.set(key, …)` for declared props with the proxy.
- **Generated settings schema** — derive the settings schema from `<props>` when the
  component is built (`crates/core/shell/src/shell/component/runtime.rs` /
  `component.rs`), so the settings UI reads it instead of `mesh.settings`.
- **Tests**: parser unit tests (`parser.rs` `#[cfg(test)]`), `prop()` resolution,
  precedence ordering, reactive write → re-render.

### Phase 2 — migrate `@mesh/audio-popover` (reference proof)

- `modules/frontend/audio-popover/src/main.mesh`: add `<props>` (`track_width`,
  `track_height`, `anim_ms`), reference via `prop()` in `<style>`; keep CSS
  min/max clamps. (See §8.)
- `modules/frontend/audio-popover/module.json`: delete the `surface` **sizing**
  fields (`width`, `height`, `size`, `prefers_content_children_sizing`,
  `min_*`/`max_*`, `display_transition`); keep only placement
  (`anchor`/`layer`/`keyboard_mode`/`visible_on_start`) — or none, as an embeddable
  popover.
- `modules/frontend/audio-popover/config/i18n/*.json`: add the `var.*` label keys.
- Verify content-measure + clamp + show/hide animation still match current
  behavior (40×200-equivalent).

### Phase 3 — narrow the manifest, delete old fields (no compat) ✅ landed

Implemented: `SurfaceLayoutSection` and `SurfaceLayoutSettings` carry placement
only; surface size is `measure_content_size()` reading the component root's CSS
box; the show/hide transition is a CSS `transition` on the root read by
`hide_transition_ms()`; `SettingsSection`/`mesh.settings` is removed; the five
shipped surfaces and the LSP manifest schema are updated. The `size` type's
`prop()` projection is Phase 1; the remaining prop types are Phase 4.

- `crates/core/extension/module/src/manifest/model.rs`: remove `width`, `height`,
  `size_policy`/`size`, `prefers_content_children_sizing`, `min_*`/`max_*`,
  `display_transition` from `SurfaceLayoutSection` (`model.rs:748`); delete
  `SettingsSection` (`model.rs:404`) and its field on `Manifest` (`model.rs:22`).
- `crates/core/surface-config/src/lib.rs`: drop the removed fields from
  `SurfaceLayoutSettings` and `surface_layout_from_manifest()` (`lib.rs:69`);
  surface size now comes from content measurement.
- Fix all readers (grep for the removed fields): the shell surface-sizing path in
  `crates/core/shell/src/shell.rs`, render/presentation size handoff, and any
  fixtures.
- Update every `module.json` that declared the removed fields (e.g.
  `modules/frontend/navigation-bar` keeps `exclusive_zone`/placement, sizing
  removed) and the LSP manifest schema in `crates/tools/lsp/src/manifest/schema.rs`
  (keep it synced with the structs per `project_lsp_manifest_support`).
- Update fixtures/tests; run the shell suite to green.

### Phase 4 — remaining types, per-instance settings storage, LSP

- **Types**: implement `number`/`int`/`bool`/`enum`/`string`/`color`/`token`/
  `duration`/`icon` projection + type-checked validation in the component compiler
  (extend Phase 1's `size` paths). `icon` resolves via `crates/core/ui/icon` and
  feeds `iconRequirements`; `token` resolves via the theme engine.
- **Per-instance settings persistence**: read/write the §9.1 shape in
  `crates/core/surface-config/src/lib.rs` (`load_frontend_module_settings`,
  `lib.rs:122`) and `crates/core/config`; key by the composition instance key.
- **LSP**: `crates/tools/lsp/src/analyzer/style.rs` — `prop(<name>)` completion +
  type-scoped value completion; `analyzer/script.rs` — `props.<name>` (and
  `props.source`/`props.at`) completion; `document.rs`/`diagnostics.rs`/`hover.rs`
  — parse `<props>`, surface type diagnostics, hover the type/default/label.

Cross-cutting: every phase adds/updates `#[cfg(test)]` tests in the touched crates;
no phase leaves an old config path readable.
