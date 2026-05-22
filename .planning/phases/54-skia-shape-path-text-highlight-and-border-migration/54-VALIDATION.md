---
phase: 54
slug: skia-shape-path-text-highlight-and-border-migration
status: complete
nyquist_compliant: true
wave_0_complete: true
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

Local verification used Nix store linker paths because the default environment did not expose `freetype` or `fontconfig` to `cc`:

```bash
export LIBRARY_PATH=/nix/store/kkgs1h2qidn50b5c5gndjrjz3v54jrq1-freetype-2.13.3/lib:/nix/store/mw89hwpv8x37px7dh1l0csnz7yv4iln2-fontconfig-2.17.1-lib/lib
export LD_LIBRARY_PATH=$LIBRARY_PATH
```

## Per-Task Verification Map

| Task ID | Plan | Requirement | Test Type | Automated Command | File Exists | Status |
|---|---|---|---|---|---|---|
| 54-01-01 | 01 | SKIA-01 | unit | `cargo test -p mesh-core-render skia_shape -- --nocapture` | yes | green |
| 54-02-01 | 02 | SKIA-01, SKIA-04 | unit | `cargo test -p mesh-core-render skia_border -- --nocapture` | yes | green |
| 54-03-01 | 03 | SKIA-01 | unit | `cargo test -p mesh-core-render skia_path -- --nocapture` | yes | green |
| 54-04-01 | 04 | TEXT-01 | unit | `cargo test -p mesh-core-render skia_text_highlight -- --nocapture` | yes | green |
| 54-05-01 | 05 | SKIA-01, SKIA-04, TEXT-01 | integration | full suite plus backend-neutrality grep | yes | green |

## Wave 0 Requirements

- [x] Pixel tests exist before removing/fencing software primitive fallbacks.
- [x] Skia path execution has fill and stroke coverage.
- [x] Selection highlight behavior stays compatible with theme-owned colors.
- [x] Retained display-list and render-object data remain Skia-free.

## Validation Sign-Off

- [x] All tasks have automated verification commands.
- [x] No watch-mode flags.
- [x] Feedback latency < 60s for targeted commands.
- [x] `nyquist_compliant: true` set after final full suite passes.
