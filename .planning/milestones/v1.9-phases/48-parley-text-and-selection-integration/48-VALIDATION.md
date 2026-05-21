---
phase: 48
slug: parley-text-and-selection-integration
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-05-18
---

# Phase 48 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `#[test]` (cargo test) |
| **Config file** | none |
| **Quick run command** | `cargo test -p mesh-core-render` |
| **Full suite command** | `cargo test -p mesh-core-render && cargo test -p mesh-core-render --features renderer-parley` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p mesh-core-render`
- **After every plan wave:** Run `cargo test -p mesh-core-render && cargo test -p mesh-core-render --features renderer-parley`
- **Before `/gsd-verify-work`:** Both default and renderer-parley feature paths must be green
- **Max feedback latency:** ~60 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 48-01-01 | 01 | 1 | TEXT-01 | — | N/A | unit | `cargo test -p mesh-core-render --features renderer-parley parley_adapter` | ❌ W0 | ⬜ pending |
| 48-01-02 | 01 | 1 | TEXT-01 | — | N/A | unit | `cargo test -p mesh-core-render --features renderer-parley parley_shapes` | ❌ W0 | ⬜ pending |
| 48-01-03 | 01 | 1 | TEXT-01 | — | N/A | compile | `cargo test -p mesh-core-render` | ✅ existing | ⬜ pending |
| 48-02-01 | 02 | 1 | TEXT-02 | — | N/A | unit | `cargo test -p mesh-core-render --features renderer-parley parley_selection` | ❌ W0 | ⬜ pending |
| 48-02-02 | 02 | 1 | TEXT-02 | — | N/A | unit | `cargo test -p mesh-core-render --features renderer-parley` | ❌ W0 | ⬜ pending |
| 48-03-01 | 03 | 2 | TEXT-03 | — | N/A | unit | `cargo test -p mesh-core-render --features renderer-parley parley_no_fonts` | ❌ W0 | ⬜ pending |
| 48-03-02 | 03 | 2 | TEXT-03 | — | N/A | compile+test | `cargo test -p mesh-core-render` | ✅ existing | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/core/frontend/render/src/parley_adapter.rs` — stubs for TEXT-01, TEXT-02, TEXT-03 (adapter module with test functions)
- [ ] Tests in `parley_adapter.rs` `#[cfg(test)]` module — `parley_shapes_text_to_lines_width_height`, `parley_selection_evidence_maps_anchor_focus`, `parley_no_fonts_emits_diagnostic_not_panic`

*Existing infrastructure (`cargo test -p mesh-core-render`) covers default-build verification; Wave 0 adds the feature-enabled test file.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Proof snapshot shows real Parley output on shipped nav/audio surfaces | TEXT-01 | Requires live shell render | Run `mesh-shell` with `renderer-parley` feature, check debug overlay proof snapshot for non-placeholder `parley_text` values on text nodes |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 60s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
