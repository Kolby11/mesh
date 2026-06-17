---
phase: 103-compositor-blur-offload
plan: 03
subsystem: render
tags: [blur, cpu-removal, no-op, compositor-offload, painter]
requires: [blur-protocol-infrastructure]
provides: [cpu-blur-removal]
affects:
  - "crates/core/frontend/render/src/surface/painter.rs"
  - "crates/core/frontend/render/src/surface/painter/tree.rs"
  - "crates/core/frontend/render/src/surface/painter/tests.rs"
tech-stack:
  added: []
  patterns: [no-op-preservation, call-site-preservation]
key-files:
  created: []
  modified:
    - "crates/core/frontend/render/src/surface/painter.rs"
    - "crates/core/frontend/render/src/surface/painter/tree.rs"
    - "crates/core/frontend/render/src/surface/painter/tests.rs"
decisions:
  - "apply_backdrop_filter and apply_backdrop_filter_in_session become no-ops with signatures preserved for future re-wiring"
  - "push_backdrop_filter_command becomes a no-op with signature preserved"
  - "Unused params prefixed with _ to suppress warnings; filter param kept unprefixed for is_none() guard"
  - "Test assertions updated to expect 0 ApplyFilter::Backdrop commands — tests now verify new behavior"
metrics:
  duration: ""
  completed_date: "2026-06-17"
---

# Phase 103 Plan 03: Remove CPU Blur Rendering Summary

**One-liner:** Made `apply_backdrop_filter` and `push_backdrop_filter_command` no-ops, removing CPU-side software blur while preserving call sites for future compositor re-wiring per BLUR-03.

## Tasks Completed

| # | Task | Commit | Key Changes |
|---|------|--------|-------------|
| 1 | Make apply_backdrop_filter methods no-ops in painter.rs | `d161e0a` | Both `apply_backdrop_filter` and `apply_backdrop_filter_in_session` now return early after `filter.is_none()` guard without calling `execute_painter_commands`. All unused params prefixed with `_`. BLUR-03 comment added. |
| 2 | Make push_backdrop_filter_command a no-op in tree.rs | `09eb1f3` | `push_backdrop_filter_command` no longer calls `commands.push(PainterCommand::ApplyFilter { ... })`. Same underscore prefix pattern and BLUR-03 comment. Three call sites unchanged and compile without modification. |

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Two painter tests asserting removed behavior**
- **Found during:** Task 2 verification (`cargo test -p mesh-core-render`)
- **Issue:** `painter_helper_lowering_routes_effect_helpers_through_command_backend` expected `commands.len() == 3` and `commands[2]` to be `ApplyFilter::Backdrop`. `painter_primitive_box_rounded_shadow_and_filters_emit_effect_classes` expected `apply_filter` in the command class list. Both assertions depended on `apply_backdrop_filter` / `push_backdrop_filter_command` pushing commands — the exact behavior we removed per BLUR-03.
- **Fix:** Updated both test assertions to match the new no-op behavior. Added BLUR-03 comments explaining why the no-op is intentional.
- **Files modified:** `crates/core/frontend/render/src/surface/painter/tests.rs`
- **Commit:** `cdd9423`

## Verification Results

### Automated Checks (All Passed)

| Check | Result |
|-------|--------|
| `cargo check -p mesh-core-render` | 0 errors, 2 pre-existing unrelated warnings |
| `cargo test -p mesh-core-render` | 136 passed, 0 failed |
| `grep -c 'ApplyFilter.*Backdrop' painter.rs` | 0 |
| `grep -c 'ApplyFilter.*Backdrop' painter/tree.rs` | 0 |
| `grep -c 'self\.apply_backdrop_filter' painter/tree.rs` | 1 (call site preserved) |
| `grep -c 'push_backdrop_filter_command' painter/tree.rs` | 3 (all call sites preserved) |

### Acceptance Criteria

| Criterion | Status |
|-----------|--------|
| `apply_backdrop_filter` is a no-op (no `execute_painter_commands` calls) | Done |
| `apply_backdrop_filter_in_session` is a no-op | Done |
| `push_backdrop_filter_command` is a no-op (no `commands.push` calls) | Done |
| All three functions retain signatures — callers compile unchanged | Done |
| `filter.is_none()` guard preserved in all three functions | Done |
| All unused params prefixed with `_` | Done |
| BLUR-03 comment in all three functions | Done |
| `cargo check -p mesh-core-render` passes | Done |
| `cargo test -p mesh-core-render` passes | Done |

## Architecture Notes

**CPU blur removal rationale:** MESH shell surfaces cannot read the compositor's framebuffer; a CPU-side "blur" was always blurring the surface's own pixels, not the content behind the window. The correct approach (plan 01-02) uses the `org_kde_kwin_blur` protocol to ask the KDE compositor to blur the region behind the surface. On non-KDE compositors the surface renders flat — which is the correct behavior since there is no blur protocol available.

**VisualFilter data flow preserved:** The `backdrop_filter: VisualFilter` field continues to flow through the display list. Plan 02's compositor blur region computation reads this field to determine which regions need blur hints sent to the compositor. The no-op functions do not discard the data — they simply do not render CPU pixels for it.

**Call site preservation:** The three call sites in `render_node_self`, `render_display_node_self`, and `append_display_node_self_paint_commands` remain unchanged. When a future plan re-wires backdrop filter effects (e.g. for non-KDE compositors using a different API), the call sites are already in place.

## Known Stubs

None. The no-op behavior is intentional per BLUR-03. CPU blur is fully removed.

## Threat Flags

None. Removing CPU blur code reduces attack surface per T-103-06 (accepted in threat model).

## Self-Check: PASSED

- [x] `crates/core/frontend/render/src/surface/painter.rs` — `apply_backdrop_filter` is no-op
- [x] `crates/core/frontend/render/src/surface/painter.rs` — `apply_backdrop_filter_in_session` is no-op
- [x] `crates/core/frontend/render/src/surface/painter/tree.rs` — `push_backdrop_filter_command` is no-op
- [x] `crates/core/frontend/render/src/surface/painter/tests.rs` — tests updated to reflect no-op behavior
- [x] Commit `d161e0a` — exists in git log
- [x] Commit `09eb1f3` — exists in git log
- [x] Commit `cdd9423` — exists in git log
- [x] `cargo check -p mesh-core-render` — zero errors
- [x] `cargo test -p mesh-core-render` — all 136 tests pass
