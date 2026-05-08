# Phase 12 Pattern Map

## Purpose

Map Phase 12 files to existing analogs so executors can follow local code patterns instead of inventing new shapes.

## File Pattern Map

| Target | Role | Closest Analog | Pattern To Reuse |
|--------|------|----------------|------------------|
| `config/themes/mesh-default-dark.json` | Theme token source | Existing `motion.*` keys in the same file | Flat JSON token keys with numeric durations, cubic-bezier strings, and default recipe strings. |
| `config/themes/mesh-default-light.json` | Theme token source | `config/themes/mesh-default-dark.json` | Keep light/dark token names synchronized. |
| `docs/theming/themes.md` | Author docs | Existing token group documentation | Explain `animation` as a flat token group and document primitive/default separation. |
| `docs/css-coverage.md` | CSS contract docs | Existing transition/animation metadata sections | Update supported examples and diagnostics wording while preserving practical-shell-CSS boundary. |
| `docs/frontend/mesh-syntax.md` | Frontend author docs | Existing style block section | Add compact examples using `token(animation.duration.fast)` and keyframes. |
| `crates/core/ui/component/src/parser/styles.rs` | CSS at-rule lowering | Existing `@container` lowering and unsupported at-rule errors | Lower supported `@keyframes`; keep unsupported forms as `ParseError::InvalidStyle`. |
| `crates/core/ui/component/src/parser.rs` | Parser regression tests | Existing `unsupported_keyframes_rule_reports_at_rule_name` and container query tests | Replace old unsupported-keyframes assertion with accept/reject matrix. |
| `crates/core/ui/elements/src/style/types.rs` | Style data model | Existing `AnimationStyle`, `TransitionStyle`, and `StyleDiagnostic` | Add keyframe-related data without broadening supported CSS properties beyond transition-safe set. |
| `crates/core/ui/elements/src/style/parse.rs` | CSS value parsing | Existing animation shorthand and transition parsing helpers | Reuse time/easing parsing and keep percentage-only keyframe stop parsing strict. |
| `crates/core/ui/elements/src/style/resolve.rs` | Token/style resolution | Existing `resolve_node_style_with_diagnostics` | Add hard-fail diagnostics for invalid animation token references. |
| `crates/core/ui/render/src/animation/keyframes.rs` | Keyframe runtime | Existing skeleton in same file | Complete `KeyframeRegistry` and `ActiveKeyframeAnimation`, but remove `from`/`to` support from TODO/implementation. |
| `crates/core/ui/render/src/animation/easing.rs` | Easing math | Existing cubic-bezier tests | Reuse `Easing` and `apply_easing` for keyframe segment progress. |
| `crates/core/ui/render/src/animation/interpolate.rs` | Interpolation primitives | Existing `Interpolate` impls | Reuse for keyframe stop interpolation. |
| `crates/core/shell/src/shell/component/animation.rs` | Shell animation integration | Current transition animation implementation | Preserve stable `_mesh_key` identity, active-map retention, and dirty marking while adding keyframe state. |
| `crates/core/shell/src/shell/component/shell_component.rs` | Render loop integration | Existing `wants_render()` and `paint()` animation calls | Include keyframe activity in render desire and dirty-frame behavior. |
| `crates/core/shell/src/shell/component/tests.rs` | Integration tests | Existing keyboard/selection/reactivity tests | Add focused animation tests using small fixtures, not full navigation-bar migration. |

## Data Flow

1. Theme files define primitive tokens and default animation recipes under `animation.*`.
2. Component style parser accepts normal style rules, container rules, and strict percentage-only keyframe rules.
3. Style resolution computes `AnimationStyle`, resolves explicit `token(animation.*)` references, and emits diagnostics for invalid references.
4. Render animation primitives interpolate keyframe stops over time.
5. Shell component state owns active animations by stable `_mesh_key` plus animation name and requests repaint while active.
6. Diagnostics are visible at compile/parse time for static keyframe errors and at runtime for unresolved animation names or token failures.

## Constraints

- Do not add `from`/`to` keyframe aliases in Phase 12.
- Do not partially run keyframes with unsupported properties.
- Do not keep `motion.*` as a long-term alias.
- Do not move the full navigation-bar proof into this phase.
