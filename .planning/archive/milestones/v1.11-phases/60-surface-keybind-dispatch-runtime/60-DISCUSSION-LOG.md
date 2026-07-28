# Phase 60: Surface Keybind Dispatch Runtime - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-05-23
**Phase:** 60-Surface Keybind Dispatch Runtime
**Areas discussed:** Runtime path, Keyboard precedence, Subscriber semantics, Proof surface

---

## Runtime Path

| Option | Description | Selected |
|--------|-------------|----------|
| Existing component input path | Reuse `FrontendSurfaceComponent` keyboard input and `dispatch_surface_shortcut` | ✓ |
| New global keybind dispatcher | Add a separate dispatcher above component input | |
| Agent discretion | Let the planner choose after deeper research | |

**User's choice:** Interactive selection unavailable; selected existing component input path by prior decision.
**Notes:** Prior milestones locked focused-surface scope and shell-owned input precedence.

---

## Keyboard Precedence

| Option | Description | Selected |
|--------|-------------|----------|
| Preserve current precedence | Keep shell-global, Tab/Escape, and Ctrl+C selection copy before surface keybinds | ✓ |
| Surface actions first | Let surface keybinds preempt more keyboard behavior | |
| Agent discretion | Let the planner choose after tests | |

**User's choice:** Interactive selection unavailable; selected preserve current precedence.
**Notes:** Text input needs extra protection from bare printable surface keybinds when an input owns focus.

---

## Subscriber Semantics

| Option | Description | Selected |
|--------|-------------|----------|
| Explicit subscribers | Dispatch only to nodes with `keybind` and `onkeybind` | ✓ |
| Manifest handler names | Let manifest actions dispatch directly to handler names | |
| Single target only | Treat duplicate subscribers as an error in Phase 60 | |

**User's choice:** Interactive selection unavailable; selected explicit subscribers.
**Notes:** Missing-target and duplicate diagnostics are Phase 62, so Phase 60 should keep dispatch deterministic and narrow.

---

## Proof Surface

| Option | Description | Selected |
|--------|-------------|----------|
| Navigation primary | Use navigation bar as the primary real-surface manifest-owned dispatch proof | ✓ |
| Audio primary | Center proof on audio popover controls | |
| Synthetic only | Avoid shipped-surface proof until Phase 64 | |

**User's choice:** Interactive selection unavailable; selected navigation primary with audio regression support.
**Notes:** Navigation already declares `mesh.keybinds.mute`; audio popover should not become redesign/UI scope.

---

## the agent's Discretion

- Helper names and file boundaries.
- Whether to start with characterization tests before production changes.

## Deferred Ideas

- Compositor-global shortcuts.
- Full keybind settings UI.
- Broad module install/resource requirement resolution.
