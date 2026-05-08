---
phase: 12
slug: theme-animation-tokens-and-css-animations
status: complete
created: 2026-05-08
requirements:
  - ANIM-01
  - ANIM-02
  - ANIM-03
  - ANIM-04
  - ANIM-05
---

# Phase 12 Research: Theme Animation Tokens and CSS Animations

## Research Complete

Phase 12 is primarily a renderer/style integration phase. The codebase already contains most of the raw pieces: theme tokens, transition metadata parsing, broad transition-safe interpolation in shell, and renderer-side animation skeleton modules. The work is to connect those pieces into a strict, author-visible keyframe system without expanding MESH into browser animation compatibility.

## Current State

### Theme Tokens

- `config/themes/mesh-default-dark.json` and `config/themes/mesh-default-light.json` already define `motion.duration.*`, `motion.easing.*`, `motion.scale.*`, `motion.opacity.*`, and `motion.default.*` tokens.
- Phase 12 context locks a hard rename to `animation.*`. The existing theme files are the migration source of truth.
- `docs/theming/themes.md` still names `motion` as a token group. It must be updated to `animation`.
- CSS examples in `docs/css-coverage.md` and `docs/frontend/mesh-syntax.md` still use raw transition values rather than the new animation token examples.

### CSS Parsing And Resolution

- `crates/core/ui/component/src/parser/styles.rs` lowers Lightning CSS rules into MESH `StyleRule` declarations. It currently rejects `LightningCssRule::Keyframes(_)` as unsupported.
- `crates/core/ui/component/src/parser.rs` has a test named `unsupported_keyframes_rule_reports_at_rule_name`, with a comment that Phase 12 owns keyframe scheduling.
- `crates/core/ui/elements/src/style/types.rs` already lists `animation`, `animation-name`, `animation-duration`, `animation-delay`, `animation-timing-function`, `animation-iteration-count`, `animation-direction`, `animation-fill-mode`, and `animation-play-state` as supported CSS properties.
- `crates/core/ui/elements/src/style/parse.rs` already parses `AnimationStyle` metadata, including `AnimationIterationCount::Infinite`, direction, fill mode, and play state.
- `crates/core/ui/elements/src/style/resolve.rs` already resolves `token(...)` and local variables for animation longhands and shorthand. Phase 12 needs stricter behavior for invalid animation token references than the general fallback path.

### Runtime Animation

- `crates/core/shell/src/shell/component/animation.rs` is the working implementation for transition interpolation. It uses stable `_mesh_key` identity, compares previous displayed styles to desired styles, writes in-flight styles into `WidgetNode::computed_style`, and sets `self.dirty = true` while animations remain active.
- The transition-safe property set is broader than the older docs: border radius, border width, opacity, background color, border color, color, width, height, min/max dimensions, padding, margin, transform, font size, letter spacing, line height, gap, and insets.
- `crates/core/shell/src/shell/component/shell_component.rs` makes `wants_render()` return true when `style_animations` is non-empty and calls `apply_style_animations(&mut tree)` before paint/metrics.
- `crates/core/ui/render/src/animation/` already contains skeleton modules for easing, interpolate, transform, transition, and keyframes. `keyframes.rs` sketches `KeyframeRule`, `KeyframeStop`, `KeyframeRegistry`, and `ActiveKeyframeAnimation`, but its TODO mentions `from`/`to`; Phase 12 context overrides this with percentage-only keyframes.

### Diagnostics

- `crates/core/ui/elements/src/style/resolve.rs` has `resolve_node_style_with_diagnostics` and emits `StyleDiagnostic` for unsupported properties and missing variables.
- Component-level diagnostics are available through `FrontendSurfaceComponent.diagnostics`, with existing patterns in `component/diagnostics.rs` and `component/runtime.rs`.
- Phase 12 should use parse/compile diagnostics for static keyframe syntax and unsupported keyframe properties, and runtime diagnostics for unresolved animation names or token failures that depend on final style/theme state.

## Implementation Strategy

### Recommended Plan Shape

1. Rename theme tokens and docs from `motion.*` to `animation.*`, preserving the flat token model and adding examples that use `token(animation.duration.fast)` and `token(animation.curves.bezier.one)`.
2. Extend style data structures so parsed components can carry strict keyframe definitions alongside normal style rules.
3. Implement percentage-only keyframe parsing and strict validation in the component parser. Reject `from`, `to`, unsupported properties, and keyframes with no transition-safe animatable properties.
4. Complete keyframe playback primitives using the existing interpolation/easing modules and the broad transition-safe style snapshot from shell transition animation.
5. Integrate active keyframe animation state into `FrontendSurfaceComponent` using stable `_mesh_key` plus animation name continuity across rebuilds.
6. Add runtime diagnostics for unresolved animation names and invalid animation token references, plus docs/tests for author-facing behavior.

### Files Most Likely To Change

- `config/themes/mesh-default-dark.json`
- `config/themes/mesh-default-light.json`
- `docs/css-coverage.md`
- `docs/theming/themes.md`
- `docs/frontend/mesh-syntax.md`
- `crates/core/ui/component/src/lib.rs`
- `crates/core/ui/component/src/parser.rs`
- `crates/core/ui/component/src/parser/styles.rs`
- `crates/core/ui/elements/src/style/types.rs`
- `crates/core/ui/elements/src/style/parse.rs`
- `crates/core/ui/elements/src/style/resolve.rs`
- `crates/core/ui/render/src/animation/transition.rs`
- `crates/core/ui/render/src/animation/keyframes.rs`
- `crates/core/ui/render/src/animation/mod.rs`
- `crates/core/shell/src/shell/component.rs`
- `crates/core/shell/src/shell/component/animation.rs`
- `crates/core/shell/src/shell/component/shell_component.rs`
- `crates/core/shell/src/shell/component/tests.rs`

## Risks And Mitigations

| Risk | Mitigation |
|------|------------|
| Keyframes restart on every reactive rebuild | Key active animations by stable `_mesh_key` plus animation name and preserve timeline when both are stable. |
| Parser accepts broad browser keyframe syntax unintentionally | Add explicit tests rejecting `from`, `to`, unsupported properties, and unsupported at-rules. |
| Invalid animation tokens silently collapse to defaults | Add strict animation-token resolution tests and runtime diagnostics. |
| Transition and keyframe interpolation diverge | Share a single animatable style snapshot or keep both implementations mechanically equivalent with tests over the same property set. |
| Infinite animations cause permanent dirty churn after removal | Retain active animations only for live `_mesh_key` entries with a resolved animation name, and remove completed finite animations. |
| Transform support appears broader than paint supports | Document that scale/rotate parse and interpolate but only translation is currently paintable until the raster path expands. |

## Validation Architecture

### Test Layers

1. Parser tests in `mesh-core-component`:
   - Accept `@keyframes pulse { 0% { opacity: 0; } 50% { opacity: 0.5; } 100% { opacity: 1; } }`.
   - Reject `from` and `to`.
   - Reject unsupported keyframe properties.
   - Reject keyframes with no transition-safe animatable properties.

2. Style resolver tests in `mesh-core-elements`:
   - Resolve `token(animation.duration.fast)` inside `animation-duration`.
   - Resolve `token(animation.curves.bezier.one)` inside `animation-timing-function`.
   - Fail hard or emit diagnostics for `token(animation.duration.fastest)`.
   - Preserve existing transition/animation shorthand parsing.

3. Renderer animation tests in `mesh-core-render`:
   - Interpolate percentage keyframe stops.
   - Honor fill mode, finite iteration counts, infinite iteration counts, direction, and paused play state.
   - Keep percentage-only semantics; no `from`/`to` aliases.

4. Shell integration tests in `mesh-core-shell`:
   - Continue a keyframe animation across rebuilds when `_mesh_key` and animation name are stable.
   - Restart/remove only when animation identity disappears or changes.
   - Mark the component dirty while active and stop redraw churn for completed finite animations.
   - Emit runtime diagnostics for unresolved animation names and invalid animation token references.

5. Documentation/config checks:
   - No `motion.` tokens remain in `config/themes/*.json` or primary animation docs.
   - Docs show `animation.default.border-radius` composed with `token(animation.duration.fast)` and `token(animation.curves.bezier.one)`.

### Commands

- Quick parser/style checks: `nix develop -c cargo test -p mesh-core-component keyframes && nix develop -c cargo test -p mesh-core-elements animation_token`
- Runtime checks: `nix develop -c cargo test -p mesh-core-render keyframe && nix develop -c cargo test -p mesh-core-shell animation`
- Full phase check: `nix develop -c cargo test -p mesh-core-component -p mesh-core-elements -p mesh-core-render -p mesh-core-shell animation`

## Planning Notes

- Phase 12 should not modify navigation-bar styling as the main proof. Phase 13 owns the comprehensive navigation-bar proof.
- A small parser/runtime fixture is preferable for Phase 12 proof coverage.
- Plan dependencies should flow from token rename and parser data shape into runtime playback and docs.
