# Phase 12: Theme Animation Tokens and CSS Animations - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md - this log preserves the alternatives considered.

**Date:** 2026-05-08
**Phase:** 12-theme-animation-tokens-and-css-animations
**Areas discussed:** Motion Token Shape, Keyframe Scope, Animation Runtime Rules, Diagnostics Boundary

---

## Motion Token Shape

| Option | Description | Selected |
|--------|-------------|----------|
| Flat tokens only | Keep `motion.duration.*`, `motion.easing.*`, and `motion.default.*`; simplest and matches current themes. | |
| Named presets only | Add named presets like `motion.preset.hover`, requiring a new schema. | |
| Both | Keep primitive tokens and add named/default presets. | ✓ |

**User's choice:** Both, but switch the namespace from `motion.*` to `animation.*`.
**Notes:** User chose a hard rename from `motion.*` to `animation.*`. User also clarified that default animations should be separate from primitive token values: primitives such as `animation.duration.fast` and `animation.curves.bezier.one`, defaults such as `animation.default.border-radius`, and recipes composed with explicit `token(...)` references.

---

## Keyframe Scope

| Option | Description | Selected |
|--------|-------------|----------|
| Transition-safe set | Allow keyframes for the same broad visual property set transitions already interpolate. | ✓ |
| Small motion set | Only `opacity`, `transform`, and maybe colors. | |
| CSS-authored but filtered | Let authors write broader keyframes, but only supported properties animate and unsupported ones emit diagnostics. | |

**User's choice:** Transition-safe set.
**Notes:** User selected percentage keyframe stops with intermediate percentages. User rejected `from`/`to` aliases for the first release. User selected token references on animation properties only, not inside keyframe stop values.

---

## Animation Runtime Rules

| Option | Description | Selected |
|--------|-------------|----------|
| Continue the running animation | Preserve timeline when `_mesh_key` plus animation name are stable. | ✓ |
| Restart on every restyle | Simpler, but can flicker or loop unexpectedly. | |
| Restart only when animation metadata changes | Continue only if name/duration/easing/etc. are unchanged. | |

**User's choice:** Continue the running animation.
**Notes:** User also selected respecting `animation-fill-mode`, supporting both finite iteration counts and `infinite`, and supporting `animation-play-state: paused` by freezing the current frame while keeping animation state.

---

## Diagnostics Boundary

| Option | Description | Selected |
|--------|-------------|----------|
| Accept and diagnose ignored properties | Supported properties animate and unsupported ones produce diagnostics. | |
| Reject the entire keyframes block | Strict validation; unsupported properties fail the block. | ✓ |
| Silently ignore unsupported properties | Least noisy, but weak diagnostics. | |

**User's choice:** Reject the entire keyframes block.
**Notes:** User initially answered `1`, then corrected the choice to `2`. User also selected rejecting non-runnable keyframes, using both compile/parse diagnostics and runtime surface diagnostics, and failing hard on invalid animation token references.

---

## the agent's Discretion

- Choose exact Rust representation for keyframe rules, stops, active animation state, diagnostics, and animation-frame scheduling.
- Choose whether to keep animation interpolation logic in shell or migrate shared pieces into `mesh-core-render`, as long as stable `_mesh_key` identity and dirty-frame behavior are preserved.

## Deferred Ideas

- Full browser animation compatibility.
- GPU transform/filter/compositing animation.
- Token references inside keyframe stop values.
- `from` and `to` keyframe aliases.
- Package/module manifest redesign, captured separately as a future phase todo.
