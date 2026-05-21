---
phase: 49
slug: anyrender-vello-paint-backend-adapter
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-05-20
---

# Phase 49 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (Rust unit tests) |
| **Config file** | Cargo.toml (workspace) |
| **Quick run command** | `cargo test -p mesh-core-render --features renderer-anyrender 2>&1 | tail -20` |
| **Full suite command** | `cargo test -p mesh-core-render --features renderer-anyrender,renderer-parley 2>&1 | tail -40` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run quick run command
- **After every plan wave:** Run full suite command
- **Before `/gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 60 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 49-01-01 | 01 | 1 | PAINT-01 | — | N/A | unit | `cargo test -p mesh-core-render --features renderer-anyrender anyrender_adapter` | ❌ W0 | ⬜ pending |
| 49-01-02 | 01 | 1 | PAINT-02 | — | N/A | unit | `cargo test -p mesh-core-render --features renderer-anyrender paint_evidence` | ❌ W0 | ⬜ pending |
| 49-01-03 | 01 | 1 | PAINT-03 | — | N/A | unit | `cargo test -p mesh-core-render --features renderer-anyrender software_painter_fallback` | ❌ W0 | ⬜ pending |
| 49-02-01 | 02 | 2 | PAINT-02 | — | N/A | unit | `cargo test -p mesh-core-render --features renderer-anyrender,renderer-parley parley_anyrender_combined` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/core/frontend/render/src/anyrender_adapter.rs` — stub module under `#[cfg(feature = "renderer-anyrender")]`
- [ ] `crates/core/frontend/render/src/tests/anyrender_tests.rs` — test stubs for PAINT-01, PAINT-02, PAINT-03

*Existing `cargo test` infrastructure covers the framework — only new test file stubs needed.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| anyrender scene op count > 0 for navigation-bar surface | PAINT-02 | Requires runtime surface rendering | Run with `renderer-anyrender` feature, inspect FocusedProofSnapshot output |
| Combined Parley+anyrender diagnostic absent when both features active | PAINT-01 | Requires runtime feature combination | Build with both features; confirm no "combined path skipped" diagnostic emitted |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 60s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
