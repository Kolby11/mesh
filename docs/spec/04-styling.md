# 04 — Styling & Theming

> Part of the [MESH Specification](README.md).

A theme is a **module with metadata plus CSS**. Scalar design values (color,
typography, spacing, radius, elevation, animation, icon style) are CSS custom
properties in `theme.css`; default styles for primitives are semantic CSS
rules. The runtime compiles that CSS into the token and component-default
model the renderer uses.

## 1. Model

1. **CSS variables are the tokens.** Themes author `--color-primary`;
   components reference `var(--color-primary)`. Token groups are prefixes
   (`color`, `spacing`, `typography`, `radius`, `elevation`, `animation`,
   `shadow`, `icon`, `font`). The versioned semantic contract below fixes public
   names and meanings; themes choose their values. Additional private tokens use
   module namespaces, and local CSS custom properties remain available.
   Unknown-token reads return `nil` and static reads outside the contract warn.
2. **One theme active at a time** (winner-takes-all — mixed token sets
   produce incoherent UI). Modes (dark / light / high-contrast) live *inside*
   a theme and are user-switchable at runtime. This is the deliberate
   opposite of icon/font/language packs, which are ordered multi-active
   chains. **Shipped:** `@mesh/mesh-default` is one theme with `dark` and
   `light` modes. Quick settings switches the active theme's mode through
   `mesh.theme.set_mode`, choosing the mode whose `color_scheme` matches. A
   theme without such a mode leaves the control disabled rather than
   switching themes.
3. **Themes are modules** (`mesh.kind: "theme"`), hot-swappable, with no
   privileged default.
4. **`node` is the inheritance root.** Every element starts from
   `ComputedStyle::default()`, inherits the theme's `node` rule, then its
   tag rule (`button`), then module-scoped defaults, then local styles.
5. **Module theme contributions are scoped.** A UI module's tokens/defaults
   apply only inside its own rendered subtree.

## 2. Theme pack shape

**Status: shipped** (CSS theme loading, modes, tokens); manifest fields per
[01 §3](01-module-system.md).

```json
{
  "name": "@alice/theme",
  "version": "0.1.0",
  "mesh": {
    "apiVersion": "0.1",
    "kind": "theme",
    "contributes": {
      "themes": [{
        "id": "alice", "label": "Alice", "tokenContract": "1.0", "default_mode": "dark",
        "modes": { "dark": "themes/dark/theme.css", "light": "themes/light/theme.css" }
      }]
    }
  }
}
```

```css
/* theme.css — tokens in :root, defaults in semantic rules */
:root {
  --color-primary: #7A5AF8;
  --color-surface: #121217;
  --spacing-md: 8;
  --radius-md: 8;
  --animation-duration-fast: 90;
  --animation-curves-bezier-standard: cubic-bezier(0.2, 0.0, 0, 1.0);
}

node   { color: var(--color-on-surface);
         transition: color var(--animation-duration-fast) var(--animation-curves-bezier-standard); }
button { background: var(--color-primary); border-radius: var(--radius-md); }
button:hover { background: var(--color-primary-container); }
```

Theme CSS supports a restricted selector surface: `:root`, `node`, semantic
element selectors, and supported pseudo-states. Module-specific selectors
belong in module scopes (§4), not global theme rules. Components compose transitions from duration and curve tokens. Legacy
`animation.default.*` shell recipes are retired and are outside the semantic
contract.

### 2.1 Semantic token contract

**Status: shipped, version 1.0.** The authoritative machine-readable vocabulary
is [semantic-tokens-v1.json](../../crates/core/foundation/theme/src/semantic-tokens-v1.json),
owned by the platform and independent of any theme stylesheet. It lists CSS
names, intended value kinds and whether the theme or resource composition
supplies them. Script names use the existing canonical dotted form, for example
`--color-on-surface` / `color.on-surface` and
`--animation-duration-short` / `animation.duration.short`.

The vocabulary covers semantic color roles (including selection), typography,
spacing, shape/radius, borders, elevation/shadows, motion/state opacity, backdrop
effects, icon sizes/axes and font roles. Theme modes provide every entry marked
`theme`; font roles and gated variable-font axes are resource-backed entries.
Base16 palette variables and retired shell recipes are outside the public
vocabulary. A theme may declare local palette variables and use them internally.

Each theme contribution declares `tokenContract: "1.0"`. Removing or changing
a role's meaning requires a new major contract; additive vocabulary is versioned
separately from theme colors and module versions. Only 1.0 is currently known.
Doctor warns for missing/unknown version declarations, incomplete declared
contracts and unknown token reads. These authoring warnings leave modules
loadable. Missing tokens retain normal runtime resolution behavior.

LSP completions use this contract and locally declared CSS variables. LSP and
`config doctor` share CSS-token and Luau-AST analysis for static `var()` and
`mesh.theme.token()` / imported theme-proxy reads, including template expressions
and CSS-capable prop defaults. Comments, quoted CSS content and unrelated Lua
strings do not count as reads. Dynamic names remain runtime lookups. Local CSS
custom properties, module-declared scoped tokens and explicitly namespaced
private references such as `@alice/widget.color.accent` are accepted without
claiming a new public semantic role.

## 3. The load-time cascade

**Status: target** (this replaces the old write-on-install model — Mesh no
longer writes module contributions into the user's theme file; any code doing
so is deleted).

The **effective theme is composed in memory at load** from three layers:

```
1. active theme pack CSS  (per active mode)
2. module mesh.theme contributions  (module-scoped, from installed manifests)
3. user token overrides  (from the settings store, 08)
```

- No compiled cache artifact; recomposition happens on theme/mode switch,
  module (un)install, and settings change, then `ThemeChanged` fires and
  surfaces re-resolve `var(--…)` on the next frame. No module code runs
  during a swap.
- Uninstalling a module simply drops its layer; remaining references to its
  tokens become unresolved-token warnings.
- The base shell defaults pack is the template and recovery point: creating a
  theme = copy base, edit `theme.css`, select in settings.

Per-node resolution order (unchanged, shipped):

```
ComputedStyle::default()
→ base shell defaults → active theme node → active theme tag rule
→ module-scope node → module-scope tag rule
→ local stylesheet rules → pseudo-state rules
```

## 4. Module theme contributions (`mesh.theme`)

**Status: shipped shape; storage moves to the load-time cascade (§3).**

UI modules may declare module-owned tokens and subtree-scoped component
defaults:

```json
"theme": {
  "tokens": { "weather.color.sunny": "#F6B73C" },
  "defaults": {
    "components": {
      "base":         { "transition": "background-color var(--animation-duration-fast) var(--animation-curves-bezier-standard)" },
      "button":       { "border-radius": "var(--radius-md)" },
      "weather-chip": { "background": "var(--weather-color-sunny)" }
    }
  }
}
```

- `base` maps to the module-scoped `node` rule; tag keys override core
  primitives inside that module's subtree only; custom keys are module-local
  component defaults.
- Invalid token names, invalid properties, or unresolved `var(--…)`
  references are install/load diagnostics.
- Contributions are not theme-variant-specific in v1.
- Module tokens are referenced with their prefixed name
  (`var(--weather-color-sunny)`); root tokens stay unqualified.

## 5. User customization

Through the settings store ([08](08-settings.md)), generated UI, and CLI:

```json
"shell": {
  "theme": {
    "active": "@alice/theme",
    "mode": "dark",
    "tokens": { "color-primary": "#FF6B00" }
  }
}
```

`tokens` is the sparse user override layer of the cascade (§3). Per-component
overrides stay in component styles (`.my-widget { --color-primary:
var(--color-tertiary); }`) and cascade like CSS variables; the registry-level
token read still returns the theme value.

## 6. Consuming tokens

**Shipped:** token aliases resolve within an immutable theme revision and retain
scalar types. Unchanged snapshot clones share alias resolutions; mutation
invalidates them. The style resolver shares parsed color and numeric token
projections across frames in bounded revision caches. Node custom properties
and component props still take precedence. Indexed static color, numeric, edge, corner and dimension declarations carry
typed assignment operations; inherited text values preserve parent inheritance.
Other property-specific token lowering remains a target.

- **In `<style>`**: `var(--…)` — the only styling path components should use
  for design values.
- **From script**: the `mesh.theme` interface —
  `token(name)`, `tokens(group)`, `modes()`, `active_mode()`,
  `set_mode(name)`; events `ThemeChanged(theme_id, mode)`,
  `TokenChanged(name, value)`.
- **Props**: a `token`-typed prop exposes a controlled, theme-aware knob
  ([03 §3.2](03-components.md)).
- Icon style (axes, sizes) and font roles are theme tokens too
  (`--icon-*`, `--font-*`) — see [05 §7](05-icons.md), [06 §3](06-fonts.md).

## 6.1 Selector support & cascade order

**Shipped:** compound selectors retain their tag/class/id candidate filter when
pseudo-states change. Pseudo-state dependencies are indexed separately for
invalidation, including all states in multi-state compounds. Class dependencies
include every class atom, independently of the forward candidate bucket.
Targeted updates test changed dependencies for selector entry and exit; nodes
whose matched rules do not change retain their computed style. Inherited text
and custom-property changes still propagate to descendants.

**Shipped:** component memo entries retain pre-animation styles. Reuse checks
the theme, rule-index revision, component props, container dimensions, inherited
values and annotated node inputs before skipping a clean subtree. Lifecycle
classes and runtime value overrides remain separate from authored memo inputs.

Component `<style>` blocks support a deliberate selector subset: tag, `.class`,
`#id`, `*`, pseudo-states (`:hover`, `:focus`, `:focus-visible`, `:active`,
`:disabled`, `:checked`, the window states below, …), and compounds of those
(`button.primary:hover`).
Descendant/child combinators (`.parent .child`, `a > b`) and relational
pseudo-classes (`:has()`) are **not** supported and are rejected at compile
time with a diagnostic — scope styles with classes on the target elements
instead.

### Window-state pseudo-classes

**Status: shipped.** A `role: "window"` surface ([01 §Surfaces](01-module-system.md))
is *told* its size by the compositor, so the same component may be a 920x700
floating window one moment and a fullscreen one the next. The compositor's
`xdg_toplevel` states are projected onto the surface tree as pseudo-states,
alongside `:windowed` for the role itself:

| Selector | True when |
| --- | --- |
| `:windowed` | The surface is realized as an `xdg_toplevel` rather than shell chrome. |
| `:fullscreen` | The window covers a whole output. |
| `:maximized` | The window fills its work area. |
| `:activated` | The compositor considers the window focused. |
| `:tiled` | Any edge abuts a neighbour or a screen edge. |

These are **ambient**: they describe the surface, and because this selector
subset has no descendant combinators, every node in the tree carries them. So
`.sidebar:fullscreen` works directly — a nested element does not need its root
to pass the state down. A layer surface that was never promoted, and every
popup, matches none of them.

`:windowed` differs in kind from the other four: it is MESH's own decision, not
the compositor's, and it is the one that is true of a merely *floating* window.
The four below it are only ever true when it is. It is what a **promotable**
surface ([01 §Promotable surfaces](01-module-system.md)) restyles against to
draw its own chrome for the role it is currently in:

```css
/* One header, two controls; the role decides which is present. */
.dock-back-button { display: none; }
.pop-out-button:windowed { display: none; }
.dock-back-button:windowed { display: flex; }
```

`display: none` takes the hidden control out of layout entirely, so it is not
clickable either — preferable to hiding it visually and leaving a live target.

Driving this from CSS rather than from script state is deliberate: the role can
change from outside the component (a settings override, a compositor keybind
over the automation IPC), and a component that tracked the role in a Lua
variable would disagree with reality the moment it did.

The idiom is to declare the floating size on the base rule and let the filling
states override it:

```css
.window { width: 920px; height: 700px; min-width: 920px; min-height: 700px; }
.window:fullscreen,
.window:maximized { width: 100%; height: 100%; min-width: 0; min-height: 0; }
```

`min-width`/`max-width`/`min-height`/`max-height` take the same values as
`width`/`height` — lengths, percentages, `auto`, and `fit-content` — plus
`none`, which (like `auto`) clears the constraint. A fixed size can therefore
be capped against whatever surface it lands in with `max-width: 100%` instead
of restating the clamp in every filling state.

There is **no CSS specificity**. Matching rules apply in source order and the
last declaration wins, regardless of selector shape (`#id` does not beat
`.class`). Theme component defaults apply before all component rules, and
`@container` queries only gate whether a rule matches — they do not reorder
it.

## 7. Theme coherence enforcement

**Status: target.** Token-based theming only works ecosystem-wide if modules
actually use tokens. The installed-graph source scan flags color literals
outside `var(--…)`/`prop(…)` in component `<style>` blocks as
`hardcoded_color_in_component_style` (warn severity, LSP + `mesh doctor`).
Escape hatch: a `/* mesh-allow-literal */` trailing comment for genuinely
fixed colors (brand marks). The inverse guard — unresolved token references —
already exists.

## 8. Tooling

```
mesh themes list                  # installed theme packs + modes
mesh themes active                # current theme + mode
mesh themes set <id> [--mode m]
mesh themes tokens [group]        # dump effective (composed) tokens
mesh themes which <token-name>    # which cascade layer supplied the value
```

## 9. Element blur (`filter`)

**Status: shipped.**

`filter: blur(<len>)` blurs the element **and its whole subtree**, exactly like
a browser: the element, its background, its text, and every descendant are
rasterized into one offscreen layer, blurred as a unit, and composited back.
The blur spills past the element's own box by roughly three times the radius,
so a blurred card fades out into what is behind it instead of stopping at a
hard edge.

```css
.card.is-dismissing {
  filter: blur(6px);
}
```

What follows from the layer semantics:

- **Descendants blur with the parent.** There is no way for a child to opt out;
  paint order inside the layer is unchanged.
- **Nesting is capped.** Filters nested more than four deep paint their subtree
  unblurred and report a painter diagnostic. Each level is an offscreen plus a
  blur pass over it.
- **Radius is capped** by `shell.render.blur.max_radius` (default 96px).
  A larger radius is dropped with a diagnostic rather than rasterized.
- **Damage is layer-shaped.** Every pixel of a blurred layer depends on all the
  others, so a change anywhere inside it repaints the whole layer region, and a
  partial repaint always replays the layer's commands as a whole.

It is the most expensive style MESH can paint, because its cost scales with the
*area* covered rather than the number of elements: a 420×420 blurred subtree
costs roughly 1.3 ms per repaint on the software painter, against 0.05 ms for
the same tree unblurred. Prefer it for transient states (a dismissing card, a
modal backdrop) over permanently blurred chrome, and see
`shell.render.blur` in [08](08-settings.md) for the quality dial.

`backdrop-filter` is covered in §10.

## 10. Compositor background blur

**Status: shipped (`ext-background-effect-v1`, `org_kde_kwin_blur`, namespace
opt-in, per-element in-surface blur); backdrop root, edge clamping and the
blurred-backdrop cache are targets.**

Background ("frosted glass") blur behind a shell surface is a **compositor**
effect: a Wayland client never sees the pixels behind its own surface. MESH
computes blur regions from each element's `backdrop-filter` — rectangles, with
rounded corners approximated as bands — and hands them to the compositor:

1. **`ext-background-effect-v1`** — preferred while the compositor advertises
   the protocol's `blur` capability (Hyprland, KWin, niri). The capability can
   arrive late or be withdrawn; MESH follows it at runtime and switches
   protocol or falls back without restarting.
2. **`org_kde_kwin_blur`** — used where only the KDE global exists (older KWin,
   some wlroots setups). This is what Qt/KDE's `enableBlurBehind()` uses.

The compositor blurs on its own renderer, so these cost MESH no pixel work.

3. **Namespace opt-in** — for compositors that blur layer surfaces by their own
   rules rather than a protocol, a surface sets `mesh.surface.blur: true`.
   MESH then appends `:blur` to that surface's layer-shell namespace, so a
   single compositor rule targets every opted-in MESH surface. On Hyprland:

   ```
   # ~/.config/hypr/hyprland.conf
   decoration { blur { enabled = true } }
   layerrule = blur, :blur$
   layerrule = ignorealpha 0.2, :blur$
   ```

   The rule blurs the whole surface wherever its alpha passes the threshold,
   independent of `backdrop-filter`. The suffix is kept when a protocol is
   available, because the namespace is a stable name compositor rules match;
   on a compositor that supports either protocol, leave the rule out, or the
   rule's blur applies in addition to the regions.

For the blur to be *visible*, the surface must be translucent where you want it
(a frosted `background-color` with alpha < 1) and must not mark that area
opaque; a solid fill or an opaque `box-shadow` painted behind a translucent
fill will hide it.

**In-surface content.** *Shipped.* A compositor never sees MESH's own pixels,
so whatever MESH painted beneath a frosted element in the same surface is
blurred by MESH. The choice is made per element:

| Beneath the element | Pixel work |
| --- | --- |
| Nothing painted in the surface (typically a surface root) | None. The desktop part goes through the compositor region. |
| In-surface content, compositor protocol bound | MESH blurs the in-surface content; the compositor blurs the desktop through the exported region. |
| In-surface content, no protocol | MESH blurs the in-surface content; the desktop part is blurred only by a namespace rule. |

Blurring the two parts separately approximates one blur over both, since blur
does not commute with alpha compositing. It is the best a client can do
without reading the screen. When the painter cannot blur at all, the element
paints flat and records a diagnostic.

A partial repaint that touches an in-surface frosted element's read region (its
box inflated by three times the radius) repaints that whole region. The buffer
under the element holds the previous frame's blurred output rather than the raw
backdrop, so a smaller repaint would blur against stale or missing pixels.
Partial and full repaints therefore produce identical pixels, and a change inside
a large frosted panel costs a repaint of the panel. Frosted roots over an empty
buffer have no read region and keep small repaints small.

*Target:* the backdrop is bounded by a backdrop root (the nearest ancestor that
paints through an offscreen); edges clamp to that root instead of fading to
transparency at the surface edge; and a cached blurred backdrop makes a change
above an unchanged backdrop a blit instead of a region repaint. These follow the
[in-surface blur engine](../../.planning/todos/pending/2026-09-30-in-surface-blur-engine.md)
plan, phases 3–5.

Popovers promoted to `xdg_popup` inherit their parent layer surface's blur on
compositors that blur layer popups.

Transitions between pixel, percentage and intrinsic lengths use the layout
measurements of their source and target styles. Reversing an interrupted
transition shortens its duration by its eased progress, including repeated
reversals; the authored target is restored when the transition completes.

## 11. Liquid glass (`backdrop-filter: refract()`)

**Status: shipped (in-surface Skia runtime shader); multi-output and foreign
exclusive zones are targets.**

Compositor blur (§10) can only blur what is behind a surface; it cannot bend
it. Refraction needs pixels MESH holds itself, so it samples one **backdrop
source**: the image at `shell.render.backdrop.image`, cover-fit to the output.
A profile that uses liquid glass normally also paints that image as its
wallpaper (`@mesh/glass-wallpaper`), so what the glass bends is what is shown.

```css
.card {
    backdrop-filter: blur(6px) refract(40px 30px) dispersion(4px);
}
```

| Function | Meaning |
| --- | --- |
| `blur(<px>)` | Blur of the sampled source. |
| `refract(<depth> [<edge>])` | Inside an `edge`-wide rim the sample moves up to `depth` along the outward normal of the element's rounded rect, steepening toward the rim. `edge` defaults to `depth`. `refract(0px 0px)` paints the source unrefracted. |
| `dispersion(<px>)` | Extra red/blue separation along the same normal for chromatic fringing. |

An element with `refract()` is painted client-side under its own background
and border (`ApplyBackdropRefraction`), and is never exported as a compositor
blur region. The rim also gets a soft top-left highlight. Without a backdrop
source, the element paints only its own background.

**Placement.** Layer-shell never tells a client where its surface is, so the
shell predicts each surface's origin from its anchor, margins and the
exclusive zones of MESH's own higher-layer surfaces on the same edge. The
prediction is exact when MESH owns every exclusive zone on that edge; another
client's panel shifts the real surface and misaligns the sampled image.

**Full-output surfaces.** A top/bottom surface whose root is `height: 100%`
spans the output (anchored to all four edges), and `exclusive-zone: ignore`
(layer-shell `-1`) keeps it under every other surface's zone — the wallpaper
shape.
