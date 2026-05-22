---
phase: 53
slug: element-and-display-list-primitive-coverage
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-05-22
---

# Phase 53 Validation Strategy

## Test Infrastructure

| Property | Value |
|----------|-------|
| Framework | Rust built-in `cargo test` |
| Quick run command | `cargo test -p mesh-core-render painter_primitive -- --nocapture` |
| Full suite command | `cargo test -p mesh-core-render painter_primitive -- --nocapture && cargo test -p mesh-core-render display_list_primitive -- --nocapture && cargo test -p mesh-core-render shipped_surface_painter -- --nocapture` |
| Backend-neutrality gate | `rg "skia_safe" crates/core/frontend/render/src/display_list.rs crates/core/frontend/render/src/render_object.rs && exit 1 || exit 0` |

## Per-Task Verification Map

| Task ID | Plan | Requirement | Test Type | Automated Command | File Exists | Status |
|---|---|---|---|---|---|---|
| 53-01-01 | 01 | ELEM-01, ELEM-02 | unit | `cargo test -p mesh-core-render painter_primitive -- --nocapture` | pending | pending |
| 53-02-01 | 02 | ELEM-01, PAINT-03 | unit | `cargo test -p mesh-core-render painter_primitive_box -- --nocapture` | pending | pending |
| 53-02-02 | 02 | ELEM-01, PAINT-03 | unit | `cargo test -p mesh-core-render painter_primitive_text_debug -- --nocapture` | pending | pending |
| 53-03-01 | 03 | ELEM-01, ELEM-02 | unit | `cargo test -p mesh-core-render painter_primitive_controls -- --nocapture` | pending | pending |
| 53-04-01 | 04 | PAINT-03, ELEM-02 | integration | `cargo test -p mesh-core-render display_list_primitive -- --nocapture` | pending | pending |
| 53-04-02 | 04 | PAINT-03, ELEM-01, ELEM-02 | integration | `cargo test -p mesh-core-render shipped_surface_painter -- --nocapture` | pending | pending |

## Wave 0 Requirements

- [ ] Command-class test helper exists for reducing `PainterCommand` values into stable class names.
- [ ] Direct and retained paths have at least one equivalence fixture before production primitive changes expand.
- [ ] Backend-neutrality grep proves retained display-list/render-object data remain Skia-free.

## Manual-Only Verifications

None expected. Phase 53 should be fully automated through command-class and shipped-surface proof.

## Validation Sign-Off

- [ ] All tasks have automated verification commands.
- [ ] No watch-mode flags.
- [ ] Feedback latency < 60s for targeted commands.
- [ ] `nyquist_compliant: true` set after final full suite passes.
