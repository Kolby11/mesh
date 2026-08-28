# Embedded contribution wrappers break layout inheritance

## Goal

Make the wrapper node the compiler puts around an embedded component instance
transparent to layout, so a contributed component can size itself from the
region that hosts it.

## Evidence

`build_tree_with_state_inner` (`crates/core/frontend/compiler/src/lib.rs`)
wraps every embedded instance in a `surface` node and assigns it
`style::embedded_root_style()`. Slot contributions reach the host through
`render_slot`, which splices those wrapper nodes straight into the host's
cluster row.

Two defects, both observed by dumping the real navigation-bar tree through
`real_frontend_module_component` at a 1092x48 surface:

1. **The wrapper's assigned style does not survive into the rendered tree.**
   `embedded_root_style()` is called (verified: ten calls for the shipped bar),
   but every wrapper in the rendered tree carries plain `ComputedStyle::default()`
   values. Setting `width: Px(999)`, `align_self: Stretch`, and
   `justify_content: Center` on it changed nothing in the output. Something
   between construction and layout replaces the root's computed style, and the
   overwrite was not located. Find it first; the rest of this item depends on
   the assigned style actually applying.

2. **The wrapper has an auto cross size, so percentage sizing dies at it.**
   A contributed root written as `height: 100%` resolves its percentage against
   the wrapper, not against the host region. With the wrapper auto-sized the
   percentage does not resolve and the root falls back to content size. In the
   shipped bar this produced icon controls of 22px and 26px in a 28px row
   instead of 28px, and a clock measuring 68px tall placed at `y = -14`: outside
   its own row and outside the 40px bar, clipped only by the bar's
   `overflow: hidden`.

## Design

The wrapper is an implementation detail, not a box the author asked for, and
should be as close to absent as layout allows:

1. Fix whatever discards the assigned root style, and add a regression that
   asserts an embedded root keeps a distinctive assigned value.
2. Give the wrapper `align_self: stretch` so it takes the host region's cross
   size, making that size definite for the contributed root's percentages.
3. Keep the child centred on the wrapper's main axis, so a host with
   `align-items: center` renders exactly as it does today when the contributed
   root does not fill.
4. Consider removing the wrapper from layout entirely instead, the way a
   customizable slot already splices its children with no wrapper of its own.
   That is the cleaner end state, but it moves instance identity, memoization
   keys, and `_mesh_slot_source` annotation onto the contributed root.

## Consequence while this is open

Navigation-bar controls carry explicit `28px` sizes rather than deriving from
the bar. That duplication is the thing this item removes, and it is why
`aspect-ratio` alone did not deliver container-derived control sizing.
