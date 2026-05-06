# CSS Coverage in MESH

MESH supports practical shell CSS, not full browser CSS. The style parser accepts a focused subset, `mesh-core-elements` resolves tokens and local variables into `ComputedStyle`, layout consumes layout fields, and the renderer consumes visual fields.

Unsupported properties produce style diagnostics. Unsupported at-rules are rejected by the component parser instead of being silently ignored.

## Supported Selectors

| Feature | Status | Notes |
|---|---|---|
| Universal, tag, class, ID | Supported | `*`, `button`, `.primary`, `#main` |
| Compound selectors | Supported | Example: `button.primary` |
| Selector lists | Supported | Lowered into separate rules |
| State pseudo-classes | Supported subset | `:hover`, `:focus`, `:active`, `:disabled`, `:checked`, `:focus-visible` map to runtime element state; `:focus-visible` is a modality-aware visible-focus heuristic rather than a plain alias of `:focus` |
| Container queries | Supported subset | `@container` size conditions with `and`; `or`, `not`, style, and scroll-state queries are rejected |

Unsupported selector families include descendant/child/sibling combinators, attribute selectors, structural pseudo-classes such as `:nth-child`, and pseudo-elements.

## Supported Properties

| Area | Properties |
|---|---|
| Box model and sizing | `width`, `height`, `min-width`, `max-width`, `min-height`, `max-height`, `padding`, `padding-*`, `padding-inline`, `padding-block`, `padding-x`, `padding-y`, `margin`, `margin-*`, `margin-inline`, `margin-block`, `margin-x`, `margin-y` |
| Borders and radius | `border`, `border-color`, `border-width`, `border-*-width`, `border-radius`, `border-*-radius` |
| Visuals | `background`, `background-color`, `color`, `opacity`, `visibility` |
| Typography | `font`, `font-family`, `font-size`, `font-weight`, `font-style`, `line-height`, `letter-spacing`, `text-align`, `text-overflow`, `direction` |
| Flex layout | `display`, `flex`, `flex-direction`, `flex-wrap`, `flex-grow`, `flex-shrink`, `flex-basis`, `justify-content`, `align-items`, `align-self`, `align-content`, `gap`, `row-gap`, `column-gap`, `gap-x` |
| Overflow | `overflow`, `overflow-x`, `overflow-y` |
| Positioning | `position`, `top`, `right`, `bottom`, `left`, `inset`, `z-index` |
| Transition metadata | `transition`, `transition-property`, `transition-duration`, `transition-delay`, `transition-timing-function` |
| Transform metadata | `transform` |
| Animation metadata | `animation`, `animation-name`, `animation-duration`, `animation-delay`, `animation-timing-function`, `animation-iteration-count`, `animation-direction`, `animation-fill-mode`, `animation-play-state` |

Shorthands are practical shell shorthands rather than complete browser-compatible shorthands. Examples that are expected to work:

```css
.card {
    --surface: token(color.surface);
    background: var(--surface);
    padding: token(spacing.md);
    margin: 4px 8px;
    border: 1px solid token(color.outline);
    border-radius: token(radius.md);
    display: flex;
    flex-direction: column;
    gap: 6px;
    overflow: hidden;
    transition: background-color 150ms ease-out, border-color 150ms ease-out;
    animation: pulse 250ms ease-in-out 50ms 2 alternate both paused;
}
```

## Transition And Interpolation

`transition-timing-function` accepts `linear`, `ease`, `ease-in`,
`ease-out`, `ease-in-out`, and `cubic-bezier(x1, y1, x2, y2)`.

The current shell animator interpolates these practical visual properties:

- `background` / `background-color`
- `border-color`
- `border-radius`
- `border-width`
- `color`
- `opacity`
- `width`
- `height`
- `padding`
- `margin`
- `transform`

`transform` currently parses `translate`, `scale`, and `rotate`. Only
translation is painted and inverted for hit-testing today; scale and rotation
flow through style resolution and transition state but still render as
identity until the non-axis-aligned paint path lands.

## Tokens And Variables

`token(...)` is first-class and resolves against the active MESH theme. It works as a full declaration value and inside practical literals such as `border: 1px solid token(color.outline)`.

Local custom properties are supported for supported declarations:

```css
.surface {
    --surface: token(color.surface);
    background: var(--surface);
    padding: token(spacing.md);
}
```

Variables are local to the rule resolution path used by Phase 8; they are not a full CSS cascade model.

## Focus Styling Guidance

Use `:focus` for logical focus state that should always track the focused control, and use `:focus-visible` for the stronger keyboard-oriented ring or highlight. Pointer focus on text-entry controls still counts as `:focus-visible`; pointer focus on non-text controls does not automatically keep the stronger visible-focus styling.

## Explicitly Out Of Scope

MESH does not implement CSS Grid, floats, multicolumn layout, full media queries, arbitrary at-rules, browser box model modes, filters, `box-shadow`, gradients/images as CSS backgrounds, generated content, or full text layout controls such as `white-space` and `word-break`.

Full transform rendering is still out of scope for the current raster path.
Only translation is visually applied today.

`@keyframes` remains unsupported until Phase 12. Animation declarations are accepted as metadata only so Phase 12 can add custom keyframe scheduling and interpolation without changing author-facing declaration names.

## Engine Boundary

Parser and lowering live in `mesh-core-component`. Computed style and value resolution live in `mesh-core-elements`. Layout and paint consumption live in `mesh-core-elements` and `mesh-core-render` respectively. LSP completions mirror this contract and intentionally avoid unsupported browser CSS such as CSS Grid and filter-heavy effects.
