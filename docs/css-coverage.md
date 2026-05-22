# MESH Shell CSS Style Profile

MESH shell CSS is the bounded CSS-like style profile for XML/.mesh surfaces. It is not arbitrary HTML/CSS/DOM/browser compatibility. The parser accepts a focused authoring subset, `StyleResolver` resolves values against `mesh-core-theme` and local custom properties, layout consumes layout fields, and render/display-list code consumes backend-neutral visual fields.

Unsupported properties produce style diagnostics or parser errors instead of hidden browser-style fallback behavior. The executable source of truth for property support lives next to `supported_css_properties()` in `crates/core/ui/elements/src/style/types.rs`.

## Status Vocabulary

| Status | Meaning |
|---|---|
| implemented | Parsed/resolved by the current MESH style path and represented in backend-neutral style/render data where applicable. |
| diagnostic-only | Recognized as an author-facing compatibility case, but not accepted as supported shell CSS. Authors should get diagnostics or no support claim. |
| deferred | In the bounded painter roadmap, but not promised by Phase 52. Later phases must add lowering, diagnostics, and proof before promoting it. |
| out-of-scope | Browser/Web platform behavior that MESH does not intend to implement for shell CSS. |

## Support Matrix

| Category | Status | Profile |
|---|---|---|
| color | implemented | `background`, `background-color`, and `color` resolve from literals, local variables, and theme tokens into `ComputedStyle` colors. |
| size | implemented | `width`, `height`, `min-width`, `max-width`, `min-height`, and `max-height` feed retained layout dimensions. |
| spacing | implemented | `padding`, `padding-*`, `padding-x`, `padding-y`, `padding-inline`, `padding-block`, `margin`, `margin-*`, `margin-x`, `margin-y`, `margin-inline`, and `margin-block` resolve into edge values. |
| border | implemented | `border`, `border-color`, `border-width`, and `border-*-width` resolve into backend-neutral border color and width fields. |
| border | diagnostic-only | Browser-specific `border-style` is not part of the executable supported-property list; practical shell border shorthands accept solid-style syntax as a compatibility parse detail. |
| radius | implemented | `border-radius` and corner-specific radius properties resolve into `Corners`. |
| opacity | implemented | `opacity` resolves into a numeric style field and remains backend-neutral. |
| transform | implemented | `transform` parses the supported MESH transform subset. Translation is the current reliable visual/hit-test path; broader transform painting remains bounded by later painter work. |
| transform | deferred | `transform-origin` is tracked in the matrix because it is accepted by the current support list, but full origin-aware painting is not a Phase 52 promise. |
| shadow | implemented | `box-shadow` parses and resolves into backend-neutral shadow data. Skia-backed shadow execution belongs to a later painter phase. |
| filter | implemented | `filter` and `backdrop-filter` parse and resolve into backend-neutral filter data. Skia-backed layer/filter execution belongs to a later painter phase. |
| image | deferred | CSS image sources such as `background-image` are future painter-profile work and are not accepted as current supported shell CSS. |
| gradient | deferred | CSS gradient syntax such as `linear-gradient(...)` is future painter-profile work and is not accepted as current supported shell CSS. |
| animation | implemented | `animation` and its longhands store constrained animation metadata. Keyframes are percentage-only and limited to transition-safe visual properties. |
| transition | implemented | `transition` and its longhands store constrained transition metadata for supported visual properties. |
| layout | implemented | `display`, `visibility`, flex properties, `gap`, `row-gap`, `column-gap`, `gap-x`, positioning, `inset`, and `z-index` feed retained layout/render state. |
| layout | out-of-scope | CSS Grid, floats, multicolumn layout, browser box model modes, full media queries, and arbitrary layout algorithms are not MESH shell CSS. |
| font | implemented | `font`, `font-family`, `font-size`, `font-weight`, `font-style`, `line-height`, `letter-spacing`, `text-align`, `text-overflow`, and text `direction` resolve into text style fields. |
| font | out-of-scope | Browser text-flow controls such as `white-space`, `text-wrap`, and `word-break` are not part of the bounded profile. |
| selectors | implemented | Universal, tag, class, ID, compound selectors, selector lists, and the supported state pseudo-classes participate in style matching. |
| selectors | out-of-scope | Descendant, child, sibling, attribute, structural pseudo-class, and pseudo-element selector behavior is outside MESH shell CSS. |
| tokens | implemented | `token(...)` resolves through `mesh-core-theme` plus `StyleResolver` for supported declaration values. |
| custom properties | implemented | CSS custom properties beginning with `--` are local variables resolved by `StyleResolver`; they are not theme tokens and do not create a full browser cascade model. |

## Practical Syntax

Shorthands are practical shell shorthands rather than full browser-compatible shorthands:

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
    transition: background-color token(animation.duration.short) token(animation.curves.bezier.standard),
                border-color token(animation.duration.short) token(animation.curves.bezier.standard);
    animation: pulse 250ms ease-in-out 50ms 2 alternate both paused;
}
```

`transition-timing-function` accepts `linear`, `ease`, `ease-in`, `ease-out`, `ease-in-out`, and `cubic-bezier(x1, y1, x2, y2)`.

The current shell animator interpolates this practical visual set:

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

`@keyframes` supports numeric percentage stops over the same transition-safe visual set. `from` and `to` aliases are rejected. Unsupported keyframe properties reject the keyframes rule. Keyframe stop values do not support `token(...)` or `var(...)` in this release.

## Tokens And Custom Properties

Theme tokens are first-class MESH theme values:

```css
.surface {
    background: token(color.surface);
    padding: token(spacing.md);
}
```

Theme token resolution belongs to `mesh-core-theme` and `StyleResolver`. Token values can be full declaration values or embedded in practical literals such as `border: 1px solid token(color.outline)`.

CSS custom properties are local variables:

```css
.surface {
    --surface: token(color.surface);
    background: var(--surface);
}
```

Custom properties are not theme tokens, are not exported to the theme registry, and are not a full browser cascade or inheritance model.

## Explicit Browser CSS Exclusions

These examples are out-of-scope browser CSS and must not be documented or tested as implemented MESH shell CSS:

- `grid-template-columns`
- `float`
- `white-space`
- `container-type`
- `text-wrap`
- arbitrary HTML elements, DOM APIs, generated content, full media queries, and browser layout compatibility modes

Unsupported at-rules are rejected by the component parser where possible. Unsupported declaration names flow through style diagnostics.

## Phase 52 Boundary

Phase 52 locks the profile contract and executable support matrix. It does not migrate widget/control painting, Skia primitive execution, effects/layers/images/gradients, animation invalidation, damage policy, or backend observability. Those remain later v1.10 painter phases.

Style profile data must stay backend-neutral. Skia belongs behind the painter backend boundary, not in style/profile structs, retained display-list data, or public author-facing style APIs.
