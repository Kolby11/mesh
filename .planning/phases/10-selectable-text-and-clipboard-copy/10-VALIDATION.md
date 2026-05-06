---
phase: 10
slug: selectable-text-and-clipboard-copy
status: draft
nyquist_compliant: true
wave_0_complete: true
created: 2026-05-06
---

# Phase 10 - Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust `cargo test` |
| **Config file** | none - existing workspace test infrastructure |
| **Quick run command** | `nix develop -c cargo test -p mesh-core-shell -p mesh-core-render selection` |
| **Full suite command** | `nix develop -c cargo test` |
| **Estimated runtime** | ~90 seconds |

---

## Sampling Rate

- **After every task commit:** Run `nix develop -c cargo test -p mesh-core-shell -p mesh-core-render selection`
- **After every plan wave:** Run `nix develop -c cargo test`
- **Before `$gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 90 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 10-01-01 | 01 | 1 | TEXT-03, TEXT-04 | T-10-01 | Copy routing only activates for explicit selectable-text ownership and preserves normal control behavior otherwise | unit/integration | `nix develop -c cargo test -p mesh-core-shell selection_input_contract` | ✅ | ⬜ pending |
| 10-01-02 | 01 | 1 | TEXT-01, TEXT-04 | T-10-01 | Drag selection cannot steal pointer behavior from buttons, sliders, switches, or inputs | integration | `nix develop -c cargo test -p mesh-core-shell selection_boundaries` | ✅ | ⬜ pending |
| 10-02-01 | 02 | 2 | TEXT-01, TEXT-04 | T-10-02 | Pointer-to-range mapping clamps inside one selectable text node and rejects clipped/ellipsized targets | unit | `nix develop -c cargo test -p mesh-core-render selection_geometry` | ✅ | ⬜ pending |
| 10-02-02 | 02 | 2 | TEXT-02 | T-10-02 | Selected text paints with theme-owned colors without altering non-selected neighbors | unit/render | `nix develop -c cargo test -p mesh-core-render selection_paint` | ✅ | ⬜ pending |
| 10-03-01 | 03 | 3 | TEXT-03 | T-10-03 | Clipboard writes use explicit shell-owned plumbing and only emit the visible selected substring | integration | `nix develop -c cargo test -p mesh-core-shell selection_clipboard` | ✅ | ⬜ pending |
| 10-03-02 | 03 | 3 | TEXT-01, TEXT-02, TEXT-03, TEXT-04 | T-10-03 | Dedicated proof fixture demonstrates passive selectable text while adjacent controls remain unaffected | integration/render | `nix develop -c cargo test -p mesh-core-shell selection_fixture -p mesh-core-render selection_fixture` | ✅ | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠ flaky*

---

## Wave 0 Requirements

- Existing infrastructure covers all phase requirements.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Clipboard paste into an external app during a live Wayland session | TEXT-03 | Unit tests can verify payload routing, but not compositor clipboard interoperability end-to-end | Run the dedicated proof fixture, drag-select text, press `Ctrl+C`, then paste into an external text field in the same Wayland session and confirm the pasted payload matches the visible selection |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or existing infrastructure coverage
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all missing references
- [x] No watch-mode flags
- [x] Feedback latency < 90s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
