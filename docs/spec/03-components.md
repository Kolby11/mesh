# 03 — Components & Props

> Part of the [MESH Specification](README.md). Syntax reference:
> [`../frontend/mesh-syntax.md`](../frontend/mesh-syntax.md), element taxonomy:
> [`../frontend/elements.md`](../frontend/elements.md).

A **component** is a user-authored reusable `.mesh` unit composed from core
elements and other components. A component module exports one primary public
component and may contain private internal components and explicitly declared
public contributions. Component modules have module kind `ui`; optional
`mesh.surface` gives one default placement, and a shell profile may mount any
`ui` module as a root. See [01 §3.2](01-module-system.md#32-module-kinds).
Configuration belongs to the component's source as a typed `<props>` public
API; packaging stays in the manifest.

The [platform philosophy](00-philosophy.md#4-elements-enforce-shared-standards)
defines which behavior belongs in elements and which belongs in components.
Settings and devtools UIs use this same component model and built-in core APIs.

## 0.1. Incremental source indexing

**Status: shipped.** Installed-graph construction validates module manifests and
activation separately from frontend source indexing. The shell compiles each
primary entrypoint and extension-point contribution independently, reuses
unchanged compiled entries by manifest/source fingerprint, and records a
module- or contribution-scoped diagnostic when a `.mesh` or Luau source fails.
One invalid source therefore does not make unrelated graph-authorized modules
disappear. The failed source remains in the watch set so an edit can retry its
index entry without mutating the resolved graph.

## 1. The `.mesh` file

```
<props>                 typed, defaulted, localized configuration (§3)
<template>              XHTML-like markup: elements, {expressions}, components
<script lang="luau">    state, lifecycle, handlers (real Luau VM)
<style>                 CSS-like styling: var(--…) tokens, prop(…) references
```

**Status: shipped**, including component `<props>` (§3).

Component rules (shipped):

- Keep files small; extract list items and distinct UI blocks into components
  under `components/`, imported with
  `local ItemRow = require("./components/item-row.mesh")`. Imports are derived
  from the Luau AST; script source is preserved unchanged. Legacy
  `import ItemRow from "…"` statements are unsupported and must migrate to
  `local ItemRow = require("…")`.
- Bare non-local script assignments are public reactive members; templates
  bind `{name}`. `local` is private. Scripts and hooks can access `self`
  (`self.meta`, `self.storage`, and component event channels).
- `bind:this={ref}` is a **live reference** into the child's environment (all
  current same-thread contexts share a single Lua realm, each in its own
  `_ENV`): reads see current values, calls are synchronous, and named event
  channels cross the boundary (`ref.Changed:on(fn)`). Host internals and
  lifecycle hooks stay private.
- `refs.<name>` is the element-node analog: last-painted geometry/state reads
  plus imperative actions (`focus`, `blur`, `click`, `scroll_into_view`,
  `scroll_to`, `set_value`) routed through the real interaction paths.
- Service data comes from interface proxies (`require("mesh.audio@>=1.0")`);
  display state (icon names, labels) is computed in the component's own
  script — never injected by the core.

**Shipped incremental updates:** script writes and service-field changes whose
template readers are all recorded element bindings patch those attributes and
text in place by default.
Patched nodes still receive style, layout, accessibility, and damage updates.
The compiler resolves binding targets to typed core properties (or explicit
free-form attributes); setters report their change classes. A binding index
retains target paths between template rebuilds, so patches do not rediscover
targets by walking the tree.
Keyed native loops with fixed-shape bodies retain their ordered unique string
or numeric keys and captured item locals. Replacing a collection or mutating an
item in place patches only changed items when those keys stay unchanged. Key,
order or length changes reconcile the template. Event handlers in the body are
re-bound to the patched item's locals; nested control flow, component instances
and two-way bindings keep the rebuild path. Writes made by handlers during input
dispatch patch once the dispatch returns.
Collection snapshots still require item comparisons. Plain item-field key
references reuse validated keys when their dependencies are unchanged; complex
keys retain runtime validation. Write-time indexed collection tracking remains
a target. Template function calls can patch
element properties, with conservative invalidation for captured public state.
Readers in structural conditions, unsupported loop sources or keys, unkeyed loop bodies,
component props, or handler bindings use template evaluation and
retained diffing instead. Service updates and reactive-effect writes patch together;
failed expressions retain the last good tree and use normal failure reporting.
A service update whose changed fields no evaluated expression reads settles
effects at delivery; when they write nothing a template reads, no frame is
scheduled.
The script body runs once as setup. Templates and lexical helpers derive
presentation; `effect(fn)` runs reactive side effects with tracked public scalar
and service-field reads, conservative compound-input invalidation, per-callback
budgets, and bounded settling. Host requests and published events produced by
paint-time effects are queued at that paint boundary with the component's
origin; the shell schedules immediate work instead of waiting for another input
handler. A returned cleanup runs before replacement and on reload or unmount; the returned handle supports idempotent `cancel()`.
`render(self)` is no longer scheduled and compiled components receive a
migration diagnostic. Finer structural reconciliation remains a **target**.

**Shipped event functions:** event attributes are full Luau expressions resolving
function values, including private locals, table members and inline closures.
An outer call is deferred event-call sugar: authored arguments evaluate at
invocation, followed by event arguments; method calls retain normal `self`.
A component's own event channel can also be bound directly, for example
`onclick={self.Activated}`; dispatch forwards the event payload to its
subscribers. A `nil` value disables an optional handler; other non-function
values are diagnosed. Loop locals are captured per rendered iteration. Component callback attributes
are actual Luau functions, callable and forwardable by the child without JSON
serialization. They preserve the defining environment and grants; removal
restores the child's authored binding. Opaque runtime leases expire on owner
reload or unmount, use normal execution budgets and local diagnostics, and
admit at most 4096 live bindings per component. Stable function references and
equivalent deferred calls reuse their leases. Reassigning a referenced function
invalidates its template binding even when JSON state did not change.
Backend and interface props remain targets (§2); this function transport is
for component callbacks.

Primary pointer drag handlers (`onpointerdown`, `onpointermove`, `onpointerup`)
use logical surface coordinates and retain the pressed target through release,
including outside its bounds. Target removal ends capture. The
[syntax guide](../frontend/mesh-syntax.md#keyboard-focus-and-traversal) describes
focused keyboard input for the same controls.

**Shipped two-way value bindings:** `bind:value="variable"` on inputs and
sliders and `bind:checked="variable"` on checkboxes/switches assign user edits
through ordinary Luau callbacks in the owning component scope. Text, numeric,
and boolean values retain their types. The assignment precedes an authored
`onchange` callback, resolved at change time. Targets are initialized variable
names; reactive public members notify template readers and effects. Loop-local
snapshots, table paths, and unsupported element/attribute pairs are rejected.
`input type="range"` and `input type="checkbox"` bindings lower to the native
slider and checkbox controls. See the [syntax guide](../frontend/mesh-syntax.md#two-way-binding).

Keyframe names resolve in the node's authoring component first, then the module
entrypoint and active theme. Local/imported component source ownership is preserved
through composition, and animation source caches include that ownership so matching
names in separate components do not share rules. Animation durations use explicit
CSS time units (`240ms`, `0.24s`); bare integers are iteration counts.

### 1.1 Popovers and escape-bounds UI

**Status: shipped.** Surfaces are containers, not authoring units. An in-tree
`<popover open>` is authored inline and *deterministically* promoted to a
compositor `xdg_popup` child surface when shown (content-driven size,
compositor anchoring, grab/hover-bridge dismiss owned by the core). Embedded
popup wrappers stay out of layout flow whether open or closed; their geometry
and parent flex gaps cannot displace visible trigger content. Popup anchor,
gravity, offsets and edge constraints apply to the laid-out content rectangle.
Transparent shadow/filter buffer reserves remain outside its `xdg_surface`
window geometry and do not push visible content away from screen edges;
compositor size replies resize content while preserving those paint reserves. Small
menus are embeddable `ui` modules with no surface geometry; only
true top-level surfaces own `mesh.surface` placement.

Tooltips that do not fit inside their owning surface are promoted to a child
popup. Their enter animation repaints that popup independently; it does not
invalidate the parent surface. Inline tooltips repaint their owning surface.
The final animation sample is applied even when a tick arrives after its deadline.
Promoted popover animation advances the shared component tree, but frames with
no parent damage preserve the parent's pixels and complete GPU scene. Changes
to the trigger or other parent content still repaint the parent normally.
Covered binding writes, including timer updates, continue to patch retained nodes
during child enter/exit transitions; lifecycle classes are restored and reapplied
without forcing a template rebuild. Shape-stable service updates that miss
binding coverage may reuse unaffected authored branches during child lifecycle;
these fallback frames keep conservative retained reconciliation. Structural
writes retain the rebuild path.

## 2. Sizing — CSS is the engine

**Status: shipped.** Every surface and component is sized by CSS measurement
of its root box: `width: 100%` spans, fixed lengths pin, `fit-content`
shrinks, `min-*`/`max-*` clamp. Manifest sizing fields do not exist. The
show/hide transition is CSS on the root. Script-side calculation reads
measured geometry from `refs.*`, computes a **value**, assigns it to a prop;
CSS re-consumes it next frame — one shared named value, no geometry fight.
Imperative `refs` geometry writes are the documented last-writer-wins escape
hatch, not the default path.

For a top-level layer surface, the component root's measured border box is also
the compositor geometry. Its resolved CSS `margin` edges become layer-shell
margins, and the anchored outer edge (border-box extent plus that edge's
margin) becomes the exclusive zone. The root may set
`exclusive-zone: none` for an overlay or HUD; the default is `auto`. These
values are derived after layout, so a CSS or prop-driven size change updates
the compositor request together with the pixels. `mesh.surface` contains no
exclusive-zone or margin numbers.

### 2.1 Author-declared customizable slots

**Status: shipped.** A component may expose selected regions to profile-level
visual composition with a named slot whose mode is customizable and whose
extension-point names a versioned UI contract. Slot identity and mode are
static, and the matching host manifest record declares compatible defaults.

The persisted node is a component placement, not a serialized WidgetNode and
not another UI language. Version 1 stores a stable placement id, a public
extension-point contribution reference, literal exposed prop values, and list
order. Elements, arbitrary nesting, Luau expressions and handlers, bindings,
styles, and surface policy remain source-level authoring.

Coders change fixed structure and precise behavior in component source. Visual
editors arrange only compatible public components inside the regions the author
chose to expose. An editor is a replaceable `ui` module consuming the
generic mesh.composition interface.

## 3. The `<props>` block

**Status: shipped** for `.mesh` components. Backend and interface declarations
remain target work (§5).

One declaration per configurable value; everything else derives from it.
The block contains one ordinary Luau table literal, parsed statically with
`full_moon`. It never executes code. Use `=` fields, `{ ... }` sequential arrays,
commas or semicolons, quoted strings and normal Luau comments/escapes. Literal
string keys (`["name"] = ...`) share the identifier-key namespace. Values may be
string/finite-number/boolean literals, literal arrays, or `t("key", "fallback")`
label metadata (one string key and an optional string fallback). Unary numeric
minus and scalar parentheses are supported; globals, arbitrary calls, computed
keys, functions, interpolation and other expressions are rejected with source
spans. Defaults keep their existing type, range and CSS-domain validation.

The former `name: { type: ... }` notation and square-bracket arrays are rejected;
migrate them to `{ name = { type = ... } }` and `{ ... }` arrays. The same static
parser is exposed as `parse_prop_declarations` for future declaration consumers;
backend/interface props remain targets, without an alternate grammar.

```html
<props>
{
    width = { type = "size",  default = "fit-content", label = t("var.width") },
    density = { type = "enum",  options = {"compact", "cozy"}, default = "cozy",
                label = t("var.density") },
    show_pct = { type = "bool",  default = true, label = t("var.show_percent") },
    gap = { type = "size",  default = "var(--spacing-xs)" },
    accent = { type = "token", default = "color-primary" },
    icon = { type = "icon",  default = "audio-volume-high" },
    anim_ms = { type = "duration", default = 120, min = 0, max = 1000 },
}
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

A `token` value names a theme token in CSS (`color-primary`, `--color-primary`)
or canonical dotted (`color.primary`) form, or is a `var(--…)` reference.
Names project as `var(--color-primary)`, so `prop(accent)` resolves through the
active theme like a direct `var()` read; `var()` values, including fallbacks,
project unchanged. Hex colors and other literals are rejected; use `color` for
those. Token defaults count as static reads for the semantic-contract warnings
in [04 §2.1](04-styling.md#21-semantic-token-contract), as do `var()` defaults of
`color` and `size` props.

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
4. **User per-instance setting** — overrides the author's value at this exact
   placement, while remaining available beneath temporary runtime overrides.
5. **Script assignment** — `props.width = computed` (reactive).
6. **Imperative `refs` geometry write** — advanced, last-writer-wins.

Scripts have final control of effective props, subject to type validation and
core invariants. A runtime assignment does not overwrite a persisted user
preference; persistence requires the settings service. Scripts must be able to
inspect the underlying user value even while their assignment wins, so they
can honor it, constrain it, or temporarily override it intentionally. Neither
script writes nor imperative geometry bypass permission or surface constraints.

**Shipped — layer introspection:** `props.source(name)` → winning layer
(`"script"` while a script assignment differs from the host-resolved value);
`props.at(name, scope)` → raw value at
`"default" | "global" | "instance" | "per_instance" | "script"` or `nil`. An
unknown scope is a runtime error, and an undeclared prop answers `nil`. The
helpers are served from the `props` table's metatable, so `source` and `at`
are reserved and rejected as declared prop names.

For example, a component can read a requested width from the user layer, clamp
it to the available area, and assign the result to `props.width` while leaving
the saved request intact. Settings and inspection UI should distinguish the
saved preference from the runtime result and its winning layer.

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
{
    track_width = { type = "size", default = "20px",  label = t("var.track_width") },
    track_height = { type = "size", default = "100px", label = t("var.track_height") },
    anim_ms = { type = "duration", default = 120, min = 0, max = 600 },
}
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

1. **Phase 1 — landed.** `<props>` parsing + all three projections for the `size`
   type end-to-end (parser block, `StyleValue::Prop`, precedence resolver,
   reactive `props` table, derived settings schema).
2. **Phase 2 — landed.** Migrate shipped frontends as reference proofs.
3. **Phase 3 — landed.** Manifest narrowed to placement-only
   `SurfaceLayoutSection`; sizing/`display_transition`/`mesh.settings`
   deleted with no compat readers; LSP manifest schema synced.
4. **Phase 4 — partial.** Remaining component types, global persistence, and
   LSP completions/hover/diagnostics landed. Per-instance settings UI and
   in-script backend/interface props remain (§5).

No phase leaves an old config path readable (no-backward-compat rule).

CSS `prop()` references in stylesheets and static inline `style` attributes are
validated against the component declarations at compile time. Diagnostics span
the offending call, including when the same prop appears in several declarations.
