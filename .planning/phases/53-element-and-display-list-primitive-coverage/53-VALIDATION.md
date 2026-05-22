---
phase: 53
slug: element-and-display-list-primitive-coverage
status: complete
nyquist_compliant: true
wave_0_complete: true
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

Local verification used Nix store linker paths because the default environment did not expose `freetype` or `fontconfig` to `cc`:

```bash
export LIBRARY_PATH=/nix/store/kkgs1h2qidn50b5c5gndjrjz3v54jrq1-freetype-2.13.3/lib:/nix/store/mw89hwpv8x37px7dh1l0csnz7yv4iln2-fontconfig-2.17.1-lib/lib
export LD_LIBRARY_PATH=$LIBRARY_PATH
```

## Per-Task Verification Map

| Task ID | Plan | Requirement | Test Type | Automated Command | File Exists | Status |
|---|---|---|---|---|---|---|
| 53-01-01 | 01 | ELEM-01, ELEM-02 | unit | `cargo test -p mesh-core-render painter_primitive -- --nocapture` | yes | green |
| 53-02-01 | 02 | ELEM-01, PAINT-03 | unit | `cargo test -p mesh-core-render painter_primitive_box -- --nocapture` | yes | green |
| 53-02-02 | 02 | ELEM-01, PAINT-03 | unit | `cargo test -p mesh-core-render painter_primitive_text_debug -- --nocapture` | yes | green |
| 53-03-01 | 03 | ELEM-01, ELEM-02 | unit | `cargo test -p mesh-core-render painter_primitive_controls -- --nocapture` | yes | green |
| 53-04-01 | 04 | PAINT-03, ELEM-02 | integration | `cargo test -p mesh-core-render display_list_primitive -- --nocapture` | yes | green |
| 53-04-02 | 04 | PAINT-03, ELEM-01, ELEM-02 | integration | `cargo test -p mesh-core-render shipped_surface_painter -- --nocapture` | yes | green |

## Wave 0 Requirements

- [x] Command-class test helper exists for reducing `PainterCommand` values into stable class names.
- [x] Direct and retained paths have at least one equivalence fixture before production primitive changes expand.
- [x] Backend-neutrality grep proves retained display-list/render-object data remain Skia-free.

## Manual-Only Verifications

None expected. Phase 53 should be fully automated through command-class and shipped-surface proof.

## Validation Sign-Off

- [x] All tasks have automated verification commands.
- [x] No watch-mode flags.
- [x] Feedback latency < 60s for targeted commands.
- [x] `nyquist_compliant: true` set after final full suite passes.
