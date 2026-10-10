# One hover-intent mechanism

**Status:** design, 2026-10-10; narrowed the same day. Script timers
(`task.delay` / `spawn` / `sleep`) and typed `mesh.shell.*` calls have landed,
see [October's log](../../log/2026-10.md). What remains is hover intent.

## Problem

Two close bridges exist for the same interaction:

- The shell defers popover hides for surfaces (`POPOVER_HOVER_BRIDGE_DELAY`,
  180 ms, `runtime/request.rs`).
- Six modules run their own 400 ms `task.delay` bridge for embedded popovers,
  about 15 lines each: the navigation-bar volume, settings, theme and language
  buttons, `theme-selector` and `language-popover`.

The theme and language pairs show the cost of doing this per module. Each
popover runs its own close after a leave, so the trigger has to cancel it when
the pointer comes back: the popover fires `CloseScheduled` and the trigger calls
`cancelPendingClose()` through `bind:this`. Before the per-instance scheduler,
the two shared one timer key and got this by accident.

## Direction

Provide a single hover-intent behavior, either as popover element attributes
(`trigger="hover"`, `close-delay`) or as a shared Luau helper on top of
`task.delay`. Use one delay token for both paths. Apply the element admission
test in 00 §4 before making it an element attribute. This is the hover-intent
part of D5 in the [direction note](2026-10-10-runtime-architecture-direction.md).

## Measurement

`navigation_hover_profile` drives the workload: pointer enter and leave across
the four triggers and into their promoted popups. A change here should keep its
step p50/p90 and report the bridge lines removed from modules.
