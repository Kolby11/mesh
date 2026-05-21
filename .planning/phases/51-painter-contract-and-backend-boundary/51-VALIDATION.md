---
phase: 51
slug: painter-contract-and-backend-boundary
status: draft
nyquist_compliant: true
wave_0_complete: true
created: 2026-05-21
---

# Phase 51 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust `cargo test` |
| **Config file** | Workspace `Cargo.toml` |
| **Quick run command** | `cargo check -p mesh-core-render` |
| **Full suite command** | `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render` |
| **Estimated runtime** | ~60-180 seconds |

## Sampling Rate

- **After every task commit:** Run `cargo check -p mesh-core-render`
- **After every plan wave:** Run `env XDG_CACHE_HOME=/tmp/codex-nix-cache nix develop -c cargo test -p mesh-core-render`
- **Before `$gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 180 seconds

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 51-01-01 | 01 | 1 | PAINT-01, BACKEND-01 | — | N/A | compile/unit | `cargo check -p mesh-core-render` | ✅ | ⬜ pending |
| 51-01-02 | 01 | 1 | PAINT-01, PAINT-02 | — | N/A | unit/static | `cargo test -p mesh-core-render painter_command_contract -- --nocapture` | ✅ | ⬜ pending |
| 51-02-01 | 02 | 2 | PAINT-02, BACKEND-02 | — | N/A | unit/static | `cargo test -p mesh-core-render painter_backend_capabilities -- --nocapture` | ✅ | ⬜ pending |
| 51-03-01 | 03 | 3 | BACKEND-01, BACKEND-02 | — | N/A | docs/static | `rg "skia_safe" crates/core/frontend/render/src/display_list.rs crates/core/frontend/render/src/render_object.rs` exits non-zero | ✅ | ⬜ pending |
| 51-03-02 | 03 | 3 | PAINT-01, BACKEND-01, BACKEND-02 | — | N/A | docs/static | `rg "fill_rect_clipped|DrawRect|Vello" docs/renderer-migration.md crates/core/frontend/render/README.md` | ✅ | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

## Wave 0 Requirements

Existing infrastructure covers all phase requirements.

## Manual-Only Verifications

All phase behaviors have automated verification or static documentation checks. Pixel visual acceptance is intentionally deferred to later Skia migration phases.

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 180s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** approved 2026-05-21
