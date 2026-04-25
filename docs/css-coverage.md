# CSS Coverage in MESH

This document tracks CSS selector and property coverage in the MESH style engine, and evaluates the tradeoff between a custom CSS engine versus embedding a browser engine.

---

## CSS Selectors

### Simple Selectors

| Selector | Syntax | Status | Notes |
|---|---|---|---|
| Universal | `*` | ✅ Implemented | Matches all elements |
| Type / Tag | `button` | ✅ Implemented | Matches by tag name |
| Class | `.primary` | ✅ Implemented | Matches by class attribute |
| ID | `#main` | ✅ Implemented | Matches by id attribute |
| Compound (chained) | `button.primary` | ✅ Implemented | All parts must match |

### Pseudo-Classes

| Selector | Syntax | Status | Notes |
|---|---|---|---|
| State (user-defined) | `button:hover` | ⚠️ Parsed only | Tag is checked; state string is stored but never evaluated against element state |
| `:hover` | `a:hover` | ⚠️ Parsed only | Requires event state tracking in `ComputedStyle` |
| `:focus` | `input:focus` | ⚠️ Parsed only | Requires focus tracking |
| `:active` | `button:active` | ⚠️ Parsed only | Requires pointer state |
| `:disabled` | `input:disabled` | ⚠️ Parsed only | |
| `:checked` | `input:checked` | ⚠️ Parsed only | |
| `:first-child` | `li:first-child` | ❌ Not implemented | Requires sibling-aware matching |
| `:last-child` | `li:last-child` | ❌ Not implemented | |
| `:nth-child(n)` | `li:nth-child(2)` | ❌ Not implemented | |
| `:nth-of-type(n)` | `p:nth-of-type(2)` | ❌ Not implemented | |
| `:not(sel)` | `:not(.hidden)` | ❌ Not implemented | |
| `:is(sel)` | `:is(h1, h2)` | ❌ Not implemented | |
| `:where(sel)` | `:where(.btn)` | ❌ Not implemented | |
| `:has(sel)` | `div:has(img)` | ❌ Not implemented | Requires subtree inspection |
| `:empty` | `div:empty` | ❌ Not implemented | |
| `:root` | `:root` | ❌ Not implemented | |
| `:focus-visible` | `:focus-visible` | ❌ Not implemented | |
| `:focus-within` | `:focus-within` | ❌ Not implemented | |
| `:placeholder-shown` | `input:placeholder-shown` | ❌ Not implemented | |
| `:any-link` | `:any-link` | ❌ Not implemented | |

### Pseudo-Elements

| Selector | Syntax | Status | Notes |
|---|---|---|---|
| `::before` | `div::before` | ❌ Not implemented | Requires generated content nodes |
| `::after` | `div::after` | ❌ Not implemented | |
| `::placeholder` | `input::placeholder` | ❌ Not implemented | |
| `::selection` | `p::selection` | ❌ Not implemented | |
| `::first-line` | `p::first-line` | ❌ Not implemented | |
| `::first-letter` | `p::first-letter` | ❌ Not implemented | |

### Combinators

| Selector | Syntax | Status | Notes |
|---|---|---|---|
| Selector list (grouping) | `a, b` | ✅ Implemented | lightningcss splits into separate rules |
| Descendant | `div span` | ❌ Not implemented | Requires walking the element tree |
| Child | `div > span` | ❌ Not implemented | Requires parent reference in matching |
| Adjacent sibling | `h1 + p` | ❌ Not implemented | |
| General sibling | `h1 ~ p` | ❌ Not implemented | |
| Column combinator | `col || td` | ❌ Not implemented | Not relevant for shell UI |

### Attribute Selectors

| Selector | Syntax | Status | Notes |
|---|---|---|---|
| Has attribute | `[attr]` | ❌ Not implemented | |
| Exact match | `[attr=val]` | ❌ Not implemented | |
| Word match | `[attr~=val]` | ❌ Not implemented | |
| Prefix | `[attr^=val]` | ❌ Not implemented | |
| Suffix | `[attr$=val]` | ❌ Not implemented | |
| Substring | `[attr*=val]` | ❌ Not implemented | |
| Hyphen list | `[attr\|=val]` | ❌ Not implemented | |

### At-Rules

| Rule | Status | Notes |
|---|---|---|
| `@container` | ✅ Implemented | `and` conditions; `or` and `not` not yet supported |
| `@media` | ❌ Not implemented | lightningcss parses it but the engine rejects it |
| `@keyframes` | ❌ Not implemented | No animation system yet |
| `@font-face` | ❌ Not implemented | Font loading is hardcoded |
| `@layer` | ❌ Not implemented | |
| `@supports` | ❌ Not implemented | |
| `@import` | ❌ Not implemented | |

---

## CSS Properties

### Box Model

| Property | Status | Notes |
|---|---|---|
| `width`, `height` | ✅ | `px`, `%`, `auto`, `fit-content` |
| `min-width`, `max-width` | ✅ | `px` only |
| `min-height`, `max-height` | ✅ | `px` only |
| `padding` (all sides) | ✅ | Shorthand + individual sides |
| `padding-inline`, `padding-block` | ✅ | |
| `margin` (all sides) | ✅ | Shorthand + individual sides |
| `margin-inline`, `margin-block` | ✅ | |
| `border-width` (all sides) | ✅ | |
| `border-radius` (all corners) | ✅ | |
| `border-color` | ✅ | |
| `box-sizing` | ❌ | Always border-box semantics |
| `border` shorthand | ❌ | Must set color/width separately |
| `outline` | ❌ | |

### Visual / Decorative

| Property | Status | Notes |
|---|---|---|
| `background-color` / `background` | ✅ | Solid color only |
| `color` | ✅ | |
| `opacity` | ✅ | |
| `overflow`, `overflow-x`, `overflow-y` | ✅ | |
| `visibility` | ❌ | |
| `background-image` | ❌ | Gradients, images not supported |
| `background-size`, `background-position` | ❌ | |
| `box-shadow` | ❌ | |
| `clip-path` | ❌ | |
| `filter` | ❌ | blur, drop-shadow, etc. |
| `backdrop-filter` | ❌ | |

### Text

| Property | Status | Notes |
|---|---|---|
| `font-family` | ✅ | |
| `font-size` | ✅ | `px` only |
| `font-weight` | ✅ | |
| `font-style` | ✅ | `normal`, `italic` |
| `line-height` | ✅ | Unitless multiplier or `px` |
| `letter-spacing` | ✅ | |
| `text-align` | ✅ | `left`, `center`, `right` |
| `text-overflow` | ✅ | `clip`, `ellipsis` |
| `text-decoration` | ❌ | underline, strikethrough |
| `text-transform` | ❌ | uppercase, lowercase, capitalize |
| `text-shadow` | ❌ | |
| `white-space` | ❌ | |
| `word-break`, `overflow-wrap` | ❌ | |
| `direction` (RTL/LTR) | ❌ | `direction` is aliased to `flex-direction` |
| `writing-mode` | ❌ | |
| `font-variant` | ❌ | |
| `line-clamp` | ❌ | |

### Flexbox

| Property | Status | Notes |
|---|---|---|
| `display: flex` / `display: none` | ✅ | Only `flex` and `none` |
| `flex-direction` | ✅ | `row`, `column` (reverse not propagated) |
| `flex-wrap` | ✅ | `wrap`, `wrap-reverse`, `nowrap` |
| `justify-content` | ✅ | `start`, `end`, `center`, `space-between`, `space-around` |
| `align-items` | ✅ | `start`, `end`, `center`, `stretch` |
| `align-content` | ✅ | |
| `align-self` | ✅ | |
| `flex-grow`, `flex-shrink`, `flex-basis` | ✅ | |
| `flex` shorthand | ✅ | `none`, `auto`, bare number |
| `gap` | ✅ | Single value; `column-gap` aliases `gap` |
| `row-gap` | ❌ | Cross-axis gap not separate |
| `order` | ❌ | |
| `justify-self`, `justify-items` | ❌ | |

### Grid

| Property | Status | Notes |
|---|---|---|
| `display: grid` | ❌ | Not implemented |
| All `grid-*` properties | ❌ | |

### Positioning

| Property | Status | Notes |
|---|---|---|
| `position` | ❌ | `static`, `relative`, `absolute`, `fixed`, `sticky` |
| `top`, `right`, `bottom`, `left` | ❌ | |
| `z-index` | ❌ | |
| `inset` | ❌ | |

### Transforms & Animation

| Property | Status | Notes |
|---|---|---|
| `transform` | ❌ | rotate, scale, translate, matrix |
| `transform-origin` | ❌ | |
| `transition` | ❌ | |
| `animation` | ❌ | |
| `will-change` | ❌ | |

### CSS Custom Properties

| Feature | Status | Notes |
|---|---|---|
| `--var: value` declarations | ❌ | Not resolved; `var()` is parsed as `StyleValue::Var` but never looked up |
| `var(--prop)` usage | ⚠️ Parsed only | Stored but returns empty string at resolution time |
| `token(name)` (MESH extension) | ✅ | Resolved against the theme token store |

---

## Custom Engine vs Chromium Engine

### Custom engine (current approach)

**Pros**
- Minimal memory footprint — critical for a shell that runs many surfaces simultaneously
- Full control over the rendering and layout pipeline
- Tight first-class integration with MESH theme tokens
- Fast startup; no V8/Blink initialization
- No web security surface (XSS, CORS, script sandbox)
- Wayland `layer-shell` integration is straightforward
- Only implement what shell UI actually needs

**Cons**
- CSS is a very large spec — implementing it correctly is months of work
- Subtle layout bugs accumulate and are hard to test exhaustively
- Extension authors will hit missing features and need workarounds
- No ecosystem or community tooling (browser DevTools, test suites)

### Chromium-based engine (WebKitGTK, wry, Tauri webview)

**Pros**
- Complete CSS / HTML support out of the box
- Web developers can contribute instantly
- Tested by billions of users
- DevTools for debugging
- Huge component ecosystem (React, HTMX, etc.)

**Cons**
- ~200–600 MB binary + runtime memory, per surface
- Each shell surface would effectively be a browser tab — terrible for a low-overhead shell
- Hard to wire into Wayland `wlr-layer-shell` correctly
- Theme token integration requires a custom CSS bridge or JS interop
- Security surface is enormous; sandbox complexity in a shell context
- Slow startup for transient surfaces (launcher, notification popups)
- Loses the native look-and-feel of the compositor

### Verdict

**Stick with the custom engine.** A desktop shell has fundamentally different requirements than a web browser. Shell surfaces are relatively simple and well-bounded: panels, launchers, notification drawers, quick settings. The full CSS spec is largely irrelevant here.

The right approach is to implement a **well-chosen subset** rather than chasing full CSS coverage:

1. **High value, low effort:** descendant combinator, `:hover`/`:focus`/`:active` state matching, `position: absolute`, `z-index`, `transform: translate/scale/rotate`, `transition`.
2. **Medium value:** `@keyframes` + `animation`, `background-image` (gradients), `text-decoration`, `grid` layout.
3. **Low value for a shell:** `::before`/`::after`, `:nth-child`, attribute selectors, `@media` (use `@container` instead), `writing-mode`, table layout.

The gap most worth closing first is **pseudo-class state matching** (`:hover`, `:focus`, `:active`) and **`position: absolute` + `z-index`** — both are needed for overlays, tooltips, and dropdown menus that any real shell surface will require.
