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
   names and meanings; themes choose their values. Components extend it with
   their own declared tokens and shared token families (§4), each defaulting
   through contract roles, and local CSS custom properties remain available.
   Unknown-token reads return `nil` and static reads outside the contract warn.
   **Compatibility is by construction:** every token a component may read
   resolves through the contract, so any installed component works under any
   installed theme. A theme can only make a component look more or less
   tuned, never broken.
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
   apply only inside its own rendered subtree; a token family's tokens apply
   inside the subtrees of the modules that use it.
6. **The theme follows the user.** The active theme is a shared user choice
   that survives switching shells. A composition may recommend a theme; a
   profile may pin one ([08 §1.1](08-settings.md#11-profile-scope)).

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

**Status: shipped, version 1.1.** The authoritative machine-readable vocabulary
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

Each theme contribution declares the contract version it was written
against, e.g. `tokenContract: "1.1"`. Removing or changing a role's meaning
requires a new major contract. A minor version only adds roles, and every role
added after 1.0 carries a platform `default` written in older roles (1.1 adds
`--color-focus-ring` = `var(--color-primary)`, `--animation-lift-hover` and
`--animation-lift-press` = `0px`). A theme declaring any minor version of the
current major is therefore complete: roles it omits take their defaults,
resolved against that theme's own values, with `ContractDefault` provenance.
Only roles without a default are required of the declared version.

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

The **effective theme is composed in memory at load** from these layers,
later winning:

```
1. active theme pack root tokens (per active mode) + contract defaults for omitted roles
2. module and token-family mesh.theme declarations (module-scoped defaults)
3. the pack's @module blocks (the theme tuning a module or family)
4. user token overrides  (from the settings store, 08)
```

**Shipped:** the order above, contract defaults, and token-family sharing. A
module's own declaration of a name shadows the same name from a family.

- No compiled cache artifact; recomposition happens on theme/mode switch,
  module (un)install, and settings change, then `ThemeChanged` fires and
  surfaces re-resolve `var(--…)` on the next frame. No module code runs
  during a swap.
- Uninstalling a module simply drops its layer; remaining references to its
  tokens become unresolved-token warnings.
- The base shell defaults pack is the template and recovery point: creating a
  theme = copy base, edit `theme.css`, select in settings.

Per-node resolution order (shipped):

```
ComputedStyle::default()
→ base shell defaults → active theme node → active theme tag rule
→ module-scope node → module-scope tag rule
→ local stylesheet rules → pseudo-state rules → inline styles → user part rules
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
  (`var(--weather-color-sunny)`); root tokens stay unqualified. A dotted
  manifest name and its CSS property name the same token
  (`glass.rim.hover` ≡ `--glass-rim-hover`).
- **Shipped:** a module token's namespace may not be a contract group
  (`color.rim` is rejected), so it can never shadow a contract role.
- **Shipped:** each declared token's default must resolve through contract
  roles or other tokens the module can see; doctor reports
  `token_default_outside_contract` otherwise. Declared tokens are the
  module's public styling hooks.

### 4.1 Token families

**Status: shipped.** A `library` module may declare `mesh.theme.tokens` and
nothing else theme-related. Every enabled module listing it in
`mesh.uses.modules` sees those tokens in its subtree, so several components
share one vocabulary that is not part of the core contract (for example
`@mesh/glass-material`'s `--glass-rim`). Doctor and the LSP accept reads of
family tokens in the modules that use the family. A family that becomes
widely shared is a candidate for a future contract minor version.

### 4.2 Theme tuning and component internals

**Status: shipped.** A theme tunes a module or family with an `@module` block:

```css
@module "@mesh/glass-material" {
  :root { --glass-rim: #ffdcff40; }   /* declared token: stable hook */
  button { border-radius: 8px; }      /* element rule: stable */
  .pill { opacity: 0.9; }             /* class/id: the module's internals */
}
```

Declared tokens and element-type rules (with states) are the stable styling
surface. Class and id selectors reach into markup the module may rename in any
release; doctor reports `theme_styles_component_internals` for each. A value
set for a token the installed target does not declare reports
`theme_sets_undeclared_module_token`. A block for a module that is not
installed is harmless. Declared style parts (§4.4) provide a stable public
way to restyle exposed component elements.

### 4.3 Independent modules

**Status: shipped.** `mesh.theme.independent: true` declares that a module
deliberately ignores the active theme (a brand mark, an art piece). Coverage
reports list it so users learn of it before they discover it.

### 4.4 Public style parts and user stylesheets

**Status: shipped.** Native elements expose static, space-separated names with
`part="glyph volume-glyph"`. Names start with an ASCII letter or underscore and
continue with ASCII letters, digits, underscores or hyphens. Bindings, empty
names and `part` on a component instance are rejected. A part belongs to the
module authoring that element, including its local components; importing a
component does not transfer its parts to the host module. Parts are a versioned
public styling API: renaming or removing one is a compatibility change.

The user stylesheet lives in `settings.shell.theme.stylesheet` in a profile's
JSON document. Shared `shell.theme.stylesheet` can supply a fallback; a profile
string replaces it, and an empty string disables it for that profile. A stylesheet
alone does not pin the theme, and releasing a theme pin preserves the stylesheet.

```json
{
  "settings": {
    "shell": {
      "theme": {
        "stylesheet": "@mesh/navigation-bar::part(frame) { border-radius: 12px; }"
      }
    }
  }
}
```

Each rule targets one scoped module and one exposed part:
`@mesh/navigation-bar::part(glyph) { opacity: 0.8; }`. Optional pseudo-states may
follow `::part()`: `:hover`, `:focus`, `:focus-visible`, `:active`, `:disabled`,
`:checked` and the window states in §6.1; multiple states require all to match.
Class/id selectors, private element targets, combinators, imports, container
queries and keyframes are rejected in user CSS. Ordinary supported declarations,
`var()` references and custom properties retain the normal style semantics.
Rules apply in source order **after component and inline declarations**. Custom
properties are resolved before ordinary properties across the full cascade.
A part name never matches another module or an unmarked element.

Profile activation, theme selection and settings reload prepare the stylesheet
with the candidate theme. Invalid CSS retains the last-known-good live settings
and theme; removing the string's contents clears its effects. The stylesheet
limit is 256 KiB. Interaction dependencies include user rules, including layout
invalidation for geometry or custom-property changes.

LSP completes exposed module parts inside the stylesheet string in settings and
profile documents. LSP and `mesh-shell config doctor` warn when a referenced part
is missing or its module is uninstalled; those rules remain stored so restoring
or reinstalling the module restores the customization. The navigation bar
exposes `frame`, `start`, `center`, `end`, `volume-button`, `glyph` and
`volume-glyph`.

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

**Status: shipped.** Token-based theming only works ecosystem-wide if modules
actually use tokens. The installed-graph source scan flags color literals
outside `var(--…)`/`prop(…)` in component `<style>` blocks as
`hardcoded_color_in_component_style` (warn severity, LSP + `mesh doctor`).
Escape hatch: a `/* mesh-allow-literal */` trailing comment for genuinely
fixed colors (brand marks). `mesh.theme.independent: true` exempts the owning
module from this warning. Hex, named and functional colours in fills, gradients,
shadows, borders, text and local custom properties are checked, including
container rules and keyframe stops. Comments, strings, URLs, `transparent` and
`currentColor` are not fixed colours. The trailing exemption applies only to
the preceding declaration on that line; token/prop expressions are exempt.
The inverse guard — unresolved token references — already exists.

## 8. Tooling

```
mesh themes list                  # installed theme packs + modes
mesh themes active                # current theme + mode
mesh themes set <id> [--mode m]
mesh themes tokens [group]        # dump effective (composed) tokens
mesh themes which <token-name>    # which cascade layer supplied the value
```

**Shipped:** `mesh-shell theme coverage [<theme>]` (default: the active theme)
prints which modules and families the theme tunes (tokens set of declared,
element rules), which declare hooks it leaves at their defaults, independent
modules, internal selectors, and `@module` targets that are not installed. The
other commands above remain targets.

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

Rounded overflow clips constrain descendant painting and point hit testing,
including retained scrolling content. Background images and gradients follow
normalized corner radii. Partial damage repaints reproduce full-paint edge
coverage; pixels outside the damage rectangle remain untouched.

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
`mesh.theme.set_backdrop_image { path }` (`service.theme.control`) selects an
absolute image file, writing where the other appearance settings go, and
repaints every surface; `mesh.theme` state publishes it as `backdrop_image`.

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

## 12. Background gradients and box shadows

**Status: shipped.** `background` accepts a solid theme-compatible CSS color,
`none`, a supported relative `url(...)`, or `linear-gradient(...)`.
The shorthand resets both the background color and image/gradient; use
`background-color` or `background-image` to change only that part.

Linear gradients run top to bottom (optionally written `to bottom`) and accept
at least two evenly spaced color stops. Every stop uses the same CSS color parser
as theme colors, including hex, named colors, transparent, and supported functional
colors such as `rgb()`, `rgba()`, `hsl()` and `hsla()`. Functional-color commas do
not separate stops. Explicit stop positions, other directions/angles, radial
and repeating gradients, and multiple background layers are outside this profile.

`box-shadow` accepts comma-separated shadows, with the first painted on top.
Each shadow has two offsets, optional blur and spread lengths (pixels or unitless
pixel values), and an optional theme-compatible CSS color before or after the
lengths. The existing omitted-color default is half-alpha black; negative blur
is clamped to zero. `none` clears the entire list. Malformed lists clear the
shadow value rather than painting a valid prefix. `inset` is parsed but inset
painting remains unsupported. Additional shadow layers are not interpolated by
the existing single-shadow animation contract.

```css
.panel {
  background: linear-gradient(#152238, rgb(30, 60, 90), transparent);
  box-shadow: 0 2px 6px rgba(0, 0, 0, 0.3), 0 8px 20px navy;
}
```

CPU painting preserves the existing two-stop and single-shadow raster behavior.
All stops and shadows participate in retained paint signatures; all outer shadow
extents participate in damage bounds. Rounded gradients preserve exact pixels
between full and partial repainting.

## 12.1 Corner shapes (`corner-shape`)

**Status: shipped (subset).** `corner-shape` selects the curve drawn for each
`border-radius`: `round` (the default) or `scoop`, which curves the corner
inward around the box's corner point and removes that quarter disc from the
box. The shorthand takes one to four keywords in `border-radius` order;
`corner-top-left-shape` and the other three longhands set one corner. Other
CSS keywords (`bevel`, `notch`, `squircle`, `superellipse()`) resolve to
`round`.

A scooped corner clips the element's own painting: background, gradient,
border and `backdrop-filter`. Outer `box-shadow`s are kept out of the scoop,
and compositor blur regions leave it out. Borders are not stroked along the
scoop's curve, and descendants are not clipped by it.

```css
/* A concave foot: the shelf's glass sweeps out into the screen edge. */
.flare-left {
  width: 30px;
  height: 30px;
  border-top-left-radius: 30px;
  corner-shape: scoop;
}
```

Radii follow CSS overlap scaling: only adjacent radii that together exceed
their side scale down, so one corner may span the whole side.

## 13. Tooltips

**Status: shipped.** Tooltips are shell-painted overlay chrome, not elements,
so a theme styles them with one `tooltip {}` rule instead of component CSS.
Values may use `var(--…)` tokens.

```css
tooltip {
  /* chrome */
  background: var(--color-surface-container-high);
  color: var(--color-on-surface);
  border: 1px solid var(--color-outline-variant);
  border-radius: var(--radius-sm);
  padding: 5px 8px;            /* one value, or vertical horizontal */
  font-size: 12px;
  font-weight: 500;
  max-width: 320px;            /* text wrap width */
  /* placement and timing */
  position: bottom;            /* top | bottom | left | right | cursor | auto */
  gap: 6;
  delay: 300;
  cursor-offset-x: 14;
  cursor-offset-y: 18;
  animation: tooltip-enter var(--animation-duration-short) ease-out;
}
```

`background-color`, `border-color` and `border-width` are accepted as
longhands. Colours the rule omits follow `--color-surface-container`
(background), `--color-surface-container-high` (border) and
`--color-on-surface` (text). The text uses the active UI font family at line
height 1.3; the border is drawn outside the box. Metrics are clamped to the
surface's tooltip overlay reserve: `max-width` to 320px, `padding` to 12px and
`border-width` to 4px. The `animation` names a theme `@keyframes` rule whose
stops may declare `opacity` and `transform` (`scale()`, `translate*()`).

Placement resolves in three steps: the theme's `position` (the
`shell.tooltip.position` setting when the theme sets none), then the hovered
element's own `tooltip-anchor` property, then a flip to the opposite side when
the chosen side would overflow the element's clipping container:

```css
.dock-tile { tooltip-anchor: top; }
```

A layer surface reserves tooltip space only below and to the right of its
content, and drops that reserve on the axis of a bottom or right anchor; a
window reserves none. When the placed tooltip does not fit inside its surface,
the shell presents it as an `xdg_popup` anchored to the element instead of
painting it in place. The popup takes the placed side as its anchor and
gravity, keeps `gap` from the element, flips across the element when the
output edge is in the way, slides along it, and accepts no pointer input. A
bottom dock's tooltip therefore shows above its icons with either anchor.
Cursor placement always paints in place.
