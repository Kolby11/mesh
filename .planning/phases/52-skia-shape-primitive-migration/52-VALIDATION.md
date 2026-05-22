---
phase: 52
slug: skia-shape-primitive-migration
status: complete
nyquist_compliant: true
wave_0_complete: true
created: 2026-05-22
---

# Phase 52 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `cargo test` |
| **Config file** | root `Cargo.toml` |
| **Quick run command** | `cargo test -p mesh-core-elements style -- --nocapture` |
| **Full suite command** | `cargo test -p mesh-core-elements style -- --nocapture && cargo test -p mesh-core-component parser -- --nocapture` |
| **Estimated runtime** | ~30 seconds for targeted style/component gates |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p mesh-core-elements style -- --nocapture`
- **After every plan wave:** Run `cargo test -p mesh-core-elements style -- --nocapture && cargo test -p mesh-core-component parser -- --nocapture`
- **Before `$gsd-verify-work`:** Targeted style/profile/component parser suite must be green
- **Max feedback latency:** 60 seconds for targeted gates

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 52-01-01 | 01 | 1 | STYLE-01 | T-52-01 | Unsupported browser CSS remains outside the MESH profile | docs/unit | `cargo test -p mesh-core-elements style_profile -- --nocapture` | ✅ | ✅ green |
| 52-02-01 | 02 | 1 | STYLE-02 | T-52-02 | Token resolution remains through `mesh-core-theme` + `StyleResolver` | unit/fixture | `cargo test -p mesh-core-elements shipped_navigation_style -- --nocapture` | ✅ | ✅ green |
| 52-03-01 | 03 | 2 | STYLE-03 | T-52-03 | Unsupported and ambiguous web-style properties emit diagnostics | unit | `cargo test -p mesh-core-elements style_diagnostics -- --nocapture` | ✅ | ✅ green |
| 52-04-01 | 04 | 2 | STYLE-01, STYLE-02, STYLE-03 | T-52-01 / T-52-02 / T-52-03 | Shipped `.mesh` styles parse/resolve without syntax or token regressions | integration/fixture | `cargo test -p mesh-core-component parser -- --nocapture` | ✅ | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [x] Add or extend `mesh-core-elements` style-profile tests that assert the
      support matrix and `supported_css_properties()` stay synchronized.
- [x] Add shipped navigation/audio style fixture tests for token resolution and
      expected diagnostics.
- [x] Update stale `mesh-core-component` parser expectations around
      filter/backdrop-filter keyframes if the Phase 52 matrix classifies them as
      accepted metadata/deferred render behavior.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| None | STYLE-01, STYLE-02, STYLE-03 | Phase 52 is docs, parser/resolver metadata, diagnostics, and fixtures | All phase behaviors have automated verification. |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 60s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** automated gates passed on 2026-05-22
