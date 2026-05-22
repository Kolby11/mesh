---
phase: 54
slug: skia-shape-path-text-highlight-and-border-migration
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-05-22
---

# Phase 54 Validation Strategy

## Test Infrastructure

| Property | Value |
|----------|-------|
| Framework | Rust built-in `cargo test` |
| Shape command | `cargo test -p mesh-core-render skia_shape -- --nocapture` |
| Path command | `cargo test -p mesh-core-render skia_path -- --nocapture` |
| Border command | `cargo test -p mesh-core-render skia_border -- --nocapture` |
| Text-highlight command | `cargo test -p mesh-core-render skia_text_highlight -- --nocapture` |
| Full suite command | `cargo test -p mesh-core-render skia_shape -- --nocapture && cargo test -p mesh-core-render skia_path -- --nocapture && cargo test -p mesh-core-render skia_border -- --nocapture && cargo test -p mesh-core-render skia_text_highlight -- --nocapture` |
| Backend-neutrality gate | `rg "skia_safe" crates/core/frontend/render/src/display_list.rs crates/core/frontend/render/src/render_object.rs && exit 1 || exit 0` |

## Per-Task Verification Map

| Task ID | Plan | Requirement | Test Type | Automated Command | File Exists | Status |
|---|---|---|---|---|---|---|
| 54-01-01 | 01 | SKIA-01 | unit | `cargo test -p mesh-core-render skia_shape -- --nocapture` | pending | pending |
| 54-02-01 | 02 | SKIA-01, SKIA-04 | unit | `cargo test -p mesh-core-render skia_border -- --nocapture` | pending | pending |
| 54-03-01 | 03 | SKIA-01 | unit | `cargo test -p mesh-core-render skia_path -- --nocapture` | pending | pending |
| 54-04-01 | 04 | TEXT-01 | unit | `cargo test -p mesh-core-render skia_text_highlight -- --nocapture` | pending | pending |
| 54-05-01 | 05 | SKIA-01, SKIA-04, TEXT-01 | integration | full suite plus backend-neutrality grep | pending | pending |

## Wave 0 Requirements

- [ ] Pixel tests exist before removing/fencing software primitive fallbacks.
- [ ] Skia path execution has fill and stroke coverage.
- [ ] Selection highlight behavior stays compatible with theme-owned colors.
- [ ] Retained display-list and render-object data remain Skia-free.

## Validation Sign-Off

- [ ] All tasks have automated verification commands.
- [ ] No watch-mode flags.
- [ ] Feedback latency < 60s for targeted commands.
- [ ] `nyquist_compliant: true` set after final full suite passes.
