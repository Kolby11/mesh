# Phase 13 Pattern Map

## Purpose

Map Phase 13 target files to the closest local analogs so execution follows existing MESH surface and shell-test patterns.

## File Pattern Map

| Target | Role | Closest Analog | Pattern To Reuse |
|--------|------|----------------|------------------|
| `modules/frontend/navigation-bar/src/main.mesh` | Top-level shipped proof surface | Existing `main.mesh` plus the broader-but-unused classes already in the same file | Keep top-level layout and service/popover wiring in one surface file; evolve structure without moving shell integration elsewhere. |
| `modules/frontend/navigation-bar/src/components/settings-button.mesh` | Compact shell control | Current file | Preserve `40px` compact button footprint, token-driven hover/focus/active states, and button-first interaction model. |
| `modules/frontend/navigation-bar/src/components/theme-button.mesh` | Compact shell control with light motion | Current file | Reuse restrained glyph motion and theme-toggle behavior; extend only if needed for richer surface consistency. |
| `modules/frontend/navigation-bar/src/components/volume-button.mesh` | Status-aware interactive control | Current file | Reuse service-driven visible/tooltip copy and compact shell button states. |
| `modules/frontend/navigation-bar/src/components/battery-button.mesh` | Dormant passive shell-status widget | Current file | Reuse existing service-driven visible status copy and compact responsive collapse; do not expand into a new popover feature. |
| `modules/frontend/navigation-bar/src/components/meta-label.mesh` | Compact passive label | Current file | Reuse token-based small-label styling and container-query hide behavior for secondary copy. |
| `modules/frontend/navigation-bar/src/components/meta-pill.mesh` | Compact accent/status chip | Current file | Reuse pill-shaped passive accent styling if the keyframe proof needs one bounded status element. |
| `crates/core/shell/src/shell/component/tests.rs` | Real-surface integration proof | Existing `navigation_bar_*` tests | Extend the real-module test style instead of inventing a new harness; assert behavior through rendered tree, focus state, and core requests. |
| `crates/core/shell/src/shell/component/tests.rs` constrained-width tests | Responsive proof | `container_size_restyle_preserves_runtime_and_local_state` | Paint at multiple widths, then assert post-layout tree differences and state preservation. |
| `modules/frontend/navigation-bar/COMPONENTS.md` | Module-local structure docs | Current file | Keep component inventory aligned with the real shipped surface and note explicit parent/child data-flow expectations. |

## Data Flow

1. `main.mesh` owns the bar-level composition, status/control clustering, and audio-popover toggling.
2. Local components own their own script state and service bindings where already established.
3. Passive status copy on the main surface proves typography and selection through standard rendered text nodes.
4. Shell tests render the real module, drive pointer/keyboard input, and inspect the rendered tree plus resulting `CoreRequest` output.

## Constraints

- Do not move the Phase 13 proof off the primary navigation-bar surface.
- Do not turn passive status copy into a control substitute.
- Do not treat the dormant battery component as approval for a new battery feature domain.
- Do not replace real-surface tests with docs-only assertions.
