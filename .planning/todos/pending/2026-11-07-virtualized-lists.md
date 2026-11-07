# Virtualized (recycling) lists

**Status:** design only, 2026-11-07. Backlog line: "opt-in recycling/virtualized
lists" under Performance in [`docs/BACKLOG.md`](../../../docs/BACKLOG.md).

## Problem

`{#for}` builds a `WidgetNode` for every item (`frontend/compiler/src/render/mod.rs`,
`TemplateNode::For` → `build_for_children`), so a scrolling list costs build,
restyle, layout, display-list and semantics work proportional to its length,
not to what is on screen. Launcher and Settings resource lists are the
motivating cases.

## Proposed shape

- Opt in on the block, not the element: `{#for item in items key={item.id} window="48px" overscan=4}`
  (fixed item extent, in CSS px, along the parent's scroll axis). Variable
  sizes and two-axis grids are out of scope.
- The block builds only items `[first - overscan, last + overscan]` and two
  synthetic spacer boxes whose main-axis size is `skipped_items * extent`, so
  scroll extent, scrollbar and `scrollIntoView` stay exact.
- The visible range derives from the nearest scrolling ancestor's offset and
  viewport size, read from the runtime tree's scroll metrics (not layout of the
  skipped items).

## Open design questions (decide before implementing)

1. **Where the range is computed.** Scroll offset lives in the runtime tree
   after layout; the build runs before it. Options: (a) rebuild the list's
   subtree when the visible range changes, through the existing narrow
   component rebuild; (b) a native list element that owns range selection and
   patches children itself. (a) reuses machinery but a scroll tick then triggers
   a rebuild; (b) is cheaper per frame but adds a native element.
2. **Retained keyed-list patching** assumes a fixed shape ("fixed-shape keyed
   native lists" in the architecture overview). A moving window changes shape
   on scroll; the structural path must handle insert/remove at both ends
   without a full-surface rebuild.
3. **State and focus.** Item-local state, focus and hover of rows that leave the
   window are lost unless parked by key; define whether that is acceptable
   (recommended: keep the focused row alive, drop the rest).
4. **Accessibility.** Rows outside the window need `set-size` / `pos-in-set`
   on their neighbours so assistive tech sees the full list (09 spec).
5. **Scroll anchoring** when items above the window are inserted.

## Exit criteria

- A 2,000-row list builds, restyles and lays out in time proportional to the
  window; gate with the node-visit counters (`docs/performance-profiling.md`),
  not only time.
- Scroll extent, scrollbar thumb and keyboard traversal match the
  non-virtualized list for the same data (pixel and semantic-tree regressions).
- Benchmark launcher and Settings resource lists before and after, release
  profile, three alternating runs.

## Implementation findings (2026-11-07)

Option (a) was chosen. Reading the code showed these touch points and hazards:

- **Syntax.** `{#for}` headers are lexed in `ui/component/src/parser/brace.rs`
  (`parse_for_header`, `BraceKind::ForOpen`) and lowered to a `mesh-for` tag in
  `parser/markup.rs`; `key=` consumes the rest of the header, so `window=` and
  `overscan=` need the key value to be terminated first. `ForNode`
  (`ui/component/src/template.rs`) gains the option; the compiler consumers of
  `TemplateNode::For` (`compiler/src/lib.rs`, `compile.rs`, `render/mod.rs`)
  and the LSP need to accept it.
- **Build.** Window the item slice in `build_for_children`
  (`compiler/src/render/mod.rs`) and add two spacer boxes. Windowed loops must
  opt out of `patchable_keyed_loop`, whose keyed patches assume a fixed shape.
  The range reaches the builder through a new `VariableStore` method delegated
  by every wrapper store (`LayeredStore`, `style_context.rs`, `compiler/src/lib.rs`).
- **Driver.** Post-layout metrics already flow back to script state:
  `publish_element_metrics` (`component/interaction_state.rs`) runs from
  `shell_component/mod.rs` only when `retained_dirty_affects_element_metrics`
  says layout, attributes or structure changed. Two hazards: a scroll offset
  change is not one of those inputs, so the driver needs its own scroll trigger;
  and `set_host_value_with_fingerprint` bumps the mutation generation but does
  not set the state's `dirty` flag, so writing the range there does not by
  itself schedule a rebuild. The driver must write the range into the owning
  runtime's state (stamp each list wrapper with its instance key) and schedule
  the narrow rebuild explicitly.
- **Memo.** Component memo fingerprints must include the window, or a scroll
  reuses the old subtree.

Suggested order: parser and compiler with unit tests for a fixed window → store
plumbing → driver and scroll trigger with a component test that scrolls a
2,000-row list → semantics/focus → benchmark.

## Status (2026-11-07, later)

Landed, uncommitted: `window=` / `overscan=` parsing (single root element
enforced), windowed build with exact spacers, `VariableStore::virtual_window`
served from `ScriptState`, and `component/virtual_list.rs`, which after layout
reads the spacers and nearest scroller and writes the new range into the owning
instance's state, then rebuilds through `invalidate_script_state`. An
end-to-end component test scrolls a 200-row list and checks the window, scroll
offset and extent. Remaining: focus retention, set size / position in set,
per-row state, memo-hit measurement, benchmarks, adopting `window` in shipped
lists.

## Automatic windowing (2026-11-07, later)

Plain `{#for}` lists of 100+ items now window themselves when the host drives
windows (`VariableStore::supports_virtual_windows`, true for `ScriptState`) and
the body is one root element. The first build shows `24 + overscan` items; after
layout the driver (`component/virtual_list.rs`) measures the built items and
either records the item size (`VirtualRange.extent`, gap excluded) and the
visible range, or records `VirtualRange::FULL` when the items are not uniform,
the list has no scrolling ancestor, or a recorded size stops matching the
measured pitch. `FULL` makes the compiler build every item, keyed patching
included. Spacers are `skipped * (extent + gap) - gap`, so parent gaps are
exact. Known costs: one extra frame before a list settles (a short flash of the
first items for a list that is then built in full), and per-row state is lost
when a row leaves the window.

## Follow-ups landed (2026-11-07, later)

Stable item identity (key, or ordinal when unkeyed; fixed spacer identities),
`aria-setsize` / `aria-posinset` on built items, focused-row retention within 256
items, surface-sized first window (`virtual_viewport_hint`). Open: settling the
measuring frame before presentation, pruning hit tests and replay for lists that
stay fully built, and measuring the shipped lists. See the backlog.
