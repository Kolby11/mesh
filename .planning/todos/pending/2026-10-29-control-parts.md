# Styleable control parts

Backlog: "Styleable control parts" in [`docs/BACKLOG.md`](../../../docs/BACKLOG.md).

## Goal

Native controls keep their behaviour in core (value range, pointer and keyboard
input, focus, accessibility), while every pixel they draw comes from CSS on
named parts. A theme or module restyles a slider, scrollbar, input or checkbox
with ordinary rules, transitions and backdrop filters; the renderer hardcodes
no control appearance. Rejected first attempt (2026-10-29): a `slider-thumb:
glass` property selecting a painter mode — it froze one look in the renderer and
could not animate.

## Model

- **Selector.** `host::part`, where `host` is the existing single-node selector
  subset (`slider`, `.volume`, `slider.volume:active`) and `part` is a name the
  host element declares. States belong to the host (`slider:active::thumb`), as
  in CSS. One variant on `mesh_core_theme::css::Selector`, so component styles,
  theme packs and `@module` blocks share it. Unknown part names are compile
  diagnostics.
- **Nodes.** Parts are real child nodes the compiler appends to the host,
  marked with their part name. Layout, paint, damage, transitions, keyframes and
  `backdrop-filter` therefore apply unchanged.
- **Matching.** A part node never matches ordinary rules. It is styled while its
  host is resolved: the host's attributes and state match the host half of
  `host::part` rules, and the part inherits from the host. Generic traversal
  skips part nodes, as it does the synthetic `surface` node.
- **Geometry.** Core writes only what follows from behaviour: the fill's and
  lead's main-axis percentages, the anchor, and the axes for
  `orient="vertical"` (part styles are authored horizontally and transposed).
  Everything else — sizes, radii, colours, shadows, glass, motion — is CSS.
  The thumb sits in a 2px anchor at the value whose `justify-content: center`
  keeps it centred at any size, including mid-transition.
- **Input and semantics.** Hit testing resolves a part to its host; parts have
  no semantic node and are never focusable.
- **Defaults.** Each part starts from an initial style tinted with the host's
  `color`, reproducing the previous painted slider; any rule overrides it.

## Slider parts

```text
slider ─ ::track (relative) ─┬─ ::fill   (value %)
                             └─ rail (absolute, inset 0) ─┬─ lead (value %)
                                                          └─ anchor (2px) ─ ::thumb
```

The rail, lead and anchor are structure and cannot be styled.

## Order

Shipped 2026-10-29: `slider` (selector, part nodes, resolver styling,
geometry, removal of the native painter, shell restyle and transitions, the
Liquid Glass thumb); then `progress`/`meter` (`::track`, `::fill`),
`checkbox`/`radio` (`::indicator`, which draws the mark in its `color`) and
`switch` (`::track`, `::thumb`, gliding with the thumb's `margin`
transition), each with its own runtime primitive, and LSP completion of part
names. See the October log. A zero-width anchor was tried first; a node
without area does not paint its subtree.

Remaining:

1. Scrollbars (`::track`, `::thumb`), keeping the retained scroll patches
   incremental.
2. Input (`::placeholder`, `::caret`, `::selection`).

## Open questions

- Neither remaining control fits the child-node model. Scrollbar geometry is
  owned by the scroll retained path (overflow metrics, scroll-scope commands);
  the input's placeholder and caret are placed from text measured at paint
  time, and inputs draw no selection yet. Likely answer: style-only parts
  resolved with the host (`for_part`, as now) into a few typed host fields the
  painter reads, as CSS `::placeholder` has no box. Decide before starting.
- A lens that magnifies the track needs in-surface refraction; today `refract()`
  samples the backdrop image, so the pressed lens blurs and rims but does not
  magnify what is in the same surface.
